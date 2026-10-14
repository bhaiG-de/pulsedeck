import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { native, isDesktop } from "./api";
import {
  ControlRegistry,
  ControlInputError,
  type ControlHandlers,
} from "./control-state";
const registry = new ControlRegistry();
let listening: Promise<unknown> | undefined;
function ensureListener() {
  if (listening || !isDesktop) return;
  listening = listen<{
    request_id: string;
    target: string;
    action: string;
    value: unknown;
  }>("product-control", async ({ payload }) => {
    try {
      const result = await registry.invoke(
        payload.target,
        payload.action,
        payload.value,
      );
      // React state commits before a subsequent display frame. Acknowledgment
      // includes the published post-commit view state, not the requested value.
      await new Promise<void>((resolve) =>
        requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
      );
      const snapshot = registry.snapshot(payload.target);
      await native("ui_publish", { target: payload.target, snapshot });
      await native("ui_reply", {
        requestId: payload.request_id,
        result: { result: result ?? null, snapshot },
        error: null,
      });
    } catch (error) {
      await native("ui_reply", {
        requestId: payload.request_id,
        result: null,
        error: {
          code:
            error instanceof ControlInputError
              ? error.code
              : "operation_failed",
          message: String(error instanceof Error ? error.message : error),
          retryable: false,
          details:
            error instanceof ControlInputError
              ? error.details
              : {
                  hint: "Inspect ui state and the current source before retrying this action.",
                },
        },
      });
    }
  }).catch((e) => {
    listening = undefined;
    console.error("Control bridge", e);
  });
}
export function useControl(
  target: string,
  handlers: ControlHandlers,
  state: unknown,
) {
  const current = useRef(handlers);
  current.current = handlers;
  const snapshot = JSON.stringify(state);
  useEffect(() => {
    const proxies: ControlHandlers = Object.fromEntries(
      Object.keys(current.current).map((name) => [
        name,
        (value: unknown) => current.current[name](value),
      ]),
    );
    registry.register(target, proxies, JSON.parse(snapshot));
    ensureListener();
    if (isDesktop)
      void listening
        ?.then(() =>
          native("ui_publish", { target, snapshot: registry.snapshot(target) }),
        )
        .catch(console.error);
  }, [target, snapshot]);
  useEffect(
    () => () => {
      registry.remove(target);
      if (isDesktop)
        void native("ui_publish", { target, snapshot: null }).catch(
          console.error,
        );
    },
    [target],
  );
}
