export type ControlHandler = (value: unknown) => unknown | Promise<unknown>;
export type ControlHandlers = Record<string, ControlHandler>;
export class ControlInputError extends Error {
  readonly code = "invalid_arguments";
  readonly retryable = false;
  readonly details: Record<string, unknown>;
  constructor(message: string, details: Record<string, unknown>) {
    super(message);
    this.details = {
      hint: "Supply a value matching the expected type, range or allowed values shown in this error; use ui state to discover mounted targets and actions.",
      ...details,
    };
  }
}
export function controlString(value: unknown): string {
  if (typeof value !== "string")
    throw new ControlInputError("Expected text", {
      expected: { type: "string" },
      hint: "Pass a JSON string, for example --value '\"120\"'.",
    });
  return value;
}
export function controlBoolean(value: unknown): boolean {
  if (typeof value !== "boolean")
    throw new ControlInputError("Expected true or false", {
      expected: { type: "boolean" },
      valid_values: [true, false],
    });
  return value;
}
export function controlNumber(
  value: unknown,
  min: number,
  max: number,
): number {
  if (
    typeof value !== "number" ||
    !Number.isFinite(value) ||
    value < min ||
    value > max
  )
    throw new ControlInputError(`Expected a number from ${min} to ${max}`, {
      expected: { type: "number", minimum: min, maximum: max },
    });
  return value;
}
export function controlEnum<const T extends string>(
  value: unknown,
  options: readonly T[],
): T {
  if (typeof value !== "string" || !options.includes(value as T))
    throw new ControlInputError(`Expected ${options.join(" / ")}`, {
      expected: { type: "string", enum: options },
      valid_values: options,
    });
  return value as T;
}
export function controlObject(
  value: unknown,
  keys?: readonly string[],
): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value))
    throw new ControlInputError("Expected an object", {
      expected: { type: "object" },
    });
  if (keys) {
    const unknown = Object.keys(value).find((key) => !keys.includes(key));
    if (unknown)
      throw new ControlInputError(
        `Unknown field '${unknown}'; expected ${keys.join(" / ")}`,
        { argument: unknown, valid_arguments: keys },
      );
  }
  return value as Record<string, unknown>;
}
export function controlInteger(
  value: unknown,
  min: number,
  max: number,
): number {
  const n = controlNumber(value, min, max);
  if (!Number.isInteger(n))
    throw new ControlInputError(`Expected an integer from ${min} to ${max}`, {
      expected: { type: "integer", minimum: min, maximum: max },
    });
  return n;
}
export function controlDraft(
  value: unknown,
  checks: Record<string, (v: unknown) => unknown>,
): Record<string, unknown> {
  const fields = controlObject(value, Object.keys(checks));
  for (const [key, v] of Object.entries(fields)) checks[key](v);
  return fields;
}
export class ControlRegistry {
  private targets = new Map<
    string,
    { handlers: ControlHandlers; snapshot: unknown }
  >();
  register(target: string, handlers: ControlHandlers, snapshot: unknown) {
    this.targets.set(target, { handlers, snapshot });
  }
  remove(target: string) {
    this.targets.delete(target);
  }
  snapshot(target: string) {
    const entry = this.targets.get(target);
    return entry
      ? { commands: Object.keys(entry.handlers).sort(), state: entry.snapshot }
      : null;
  }
  async invoke(target: string, action: string, value: unknown) {
    const entry = this.targets.get(target);
    if (!entry) throw new Error(`UI target ${target} is unavailable`);
    const handler = entry.handlers[action];
    if (!Object.hasOwn(entry.handlers, action) || !handler)
      throw new Error(`Unsupported UI action: ${target}.${action}`);
    return await handler(value);
  }
}
