const DOTS = Array.from({ length: 25 }, (_, i) => [
  2.2 + (i % 5) * 4.4,
  2.2 + Math.floor(i / 5) * 4.4,
]).filter(([x, y]) => Math.hypot(x - 11, y - 11) < 10.5);

export default function BrandMark() {
  return (
    <svg
      className="brand-mark"
      viewBox="0 0 22 22"
      aria-hidden="true"
      data-tauri-drag-region
    >
      {DOTS.map(([x, y]) => (
        <circle
          key={`${x}-${y}`}
          cx={x}
          cy={y}
          r={1.45}
          className={x === 11 && y === 11 ? "live" : undefined}
        />
      ))}
    </svg>
  );
}
