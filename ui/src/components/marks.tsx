// A run's state on any page: its status and whether its world was restored approximately.
import { StatusMark, Tag } from '@/components/ui/status';
import { runMark } from '@/lib/mark';
import { APPROXIMATE_LABEL, isApproximate, planLine } from '@/model/restore';
import type { BranchStatus, RestoredPart } from '@/model/types';

/** A branch's status: shape + word, then colour (only when abnormal). */
export function StatusChip({ status }: { status: BranchStatus }) {
  return <StatusMark mark={runMark(status)} />;
}

/** "Approximate restore" on a branch (or compare side) some part of which was restored approximately. */
export function ApproximateMark({ of }: { of: { restored?: RestoredPart[] | null } }) {
  if (!isApproximate(of)) return null;
  return <Tag tone="warn" title={of.restored?.map(planLine).join('\n')}>{APPROXIMATE_LABEL}</Tag>;
}
