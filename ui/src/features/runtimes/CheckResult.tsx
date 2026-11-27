// One package's conformance check: passed or failed, and each step on request.
import type { RuntimeCheck } from '@/api/runtimes';
import { cn } from '@/lib/cn';

/** "Passed" / "Failed" with the first failing step, the steps behind a disclosure. */
export default function CheckResult({ check }: { check: RuntimeCheck }) {
  const failed = check.steps.find((s) => !s.ok);
  return (
    <details className="text-xs">
      <summary className="cursor-pointer">
        <span className={cn('font-medium', check.passed ? 'text-ok' : 'text-alarm')}>{check.passed ? 'Check passed' : 'Check failed'}</span>
        <span className="text-muted"> · {check.package}{failed ? ` · ${failed.name}: ${failed.detail}` : ''}</span>
      </summary>
      <ul className="mt-1 grid gap-0.5 pl-3">
        {check.steps.map((s) => (
          <li key={s.name}><span className={s.ok ? 'text-ok' : 'text-alarm'}>{s.ok ? 'ok' : 'failed'}</span> <span className="font-mono">{s.name}</span> <span className="text-muted">{s.detail}</span></li>
        ))}
        {check.skipped.map((s) => <li key={s} className="text-faint">{s}</li>)}
      </ul>
    </details>
  );
}
