import { memo } from "react";

/* Static engraved geometry; the native control value rotates only its pointer. */
const knobTicks = Array.from({ length: 11 }, (_, index) => -135 + index * 27);
const platterTicks = Array.from({ length: 48 }, (_, index) => index * 7.5);

function AnatomyDial({ platter = false }: { platter?: boolean }) {
  const ticks = platter ? platterTicks : knobTicks;
  return (
    <svg
      className={`anatomy-dial ${platter ? "anatomy-platter" : "anatomy-knob"}`}
      viewBox="0 0 100 100"
      aria-hidden="true"
      focusable="false"
    >
      {!platter && <circle className="dial-side" cx="50" cy="54" r="34" />}
      <circle className="dial-face" cx="50" cy="50" r={platter ? 46 : 34} />
      <circle className="dial-groove" cx="50" cy="50" r={platter ? 38 : 28} />
      {platter && <circle className="dial-groove" cx="50" cy="50" r="33" />}
      <g className="dial-ticks">
        {ticks.map((angle, index) => (
          <path
            key={angle}
            transform={`rotate(${angle} 50 50)`}
            d={`M50 7v${index % (platter ? 4 : 5) === 0 ? 6 : 3}`}
          />
        ))}
      </g>
      {!platter && (
        <g className="dial-pointer">
          <path d="M50 25v16" />
          <circle cx="50" cy="47" r="2" />
        </g>
      )}
    </svg>
  );
}

export default memo(AnatomyDial);
