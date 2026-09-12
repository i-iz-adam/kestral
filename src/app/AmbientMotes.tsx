/** A handful of slow-drifting embers behind the whole app. Purely
 * decorative and non-interactive — deterministic per-index placement
 * (no Math.random) so it never causes a hydration/re-render flicker,
 * just a bit of ambient "the world is alive" motion in the empty space
 * around panels. Cheap: a dozen absolutely-positioned divs animated with
 * CSS transforms/opacity only. */
export default function AmbientMotes({ count = 14 }: { count?: number }) {
  const motes = Array.from({ length: count }, (_, i) => {
    const left = (i * 71) % 100;
    const duration = 16 + (i % 6) * 3.5;
    const delay = -(i * 2.3) % duration;
    const violet = i % 3 === 0;
    return { left, duration, delay, violet, key: i };
  });

  return (
    <div className="ambient-motes" aria-hidden="true">
      {motes.map((m) => (
        <span
          key={m.key}
          className={"mote" + (m.violet ? " violet" : "")}
          style={{
            left: `${m.left}%`,
            animationDuration: `${m.duration}s`,
            animationDelay: `${m.delay}s`,
          }}
        />
      ))}
    </div>
  );
}
