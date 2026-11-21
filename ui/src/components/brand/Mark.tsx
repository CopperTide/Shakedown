// The CopperTide mark: three tide lines, the middle one copper. The outer lines follow the text
// colour, so the mark reads in both themes; the copper line never changes.

export function Mark({ className = 'size-5' }: { className?: string }) {
  return (
    <svg viewBox="0 0 120 120" className={className} aria-hidden="true" fill="none" strokeWidth={8} strokeLinecap="round">
      <path d="M20 42 C33 34 47 34 60 42 C73 50 87 50 100 42" stroke="currentColor" />
      <path d="M20 60 C33 52 47 52 60 60 C73 68 87 68 100 60" stroke="var(--copper)" />
      <path d="M20 78 C33 70 47 70 60 78 C73 86 87 86 100 78" stroke="currentColor" />
    </svg>
  );
}

/** The mark and the product name, as the sidebar shows them. */
export function Wordmark() {
  return (
    <span className="inline-flex items-center gap-2 font-semibold text-fg">
      <Mark />
      Shakedown
    </span>
  );
}
