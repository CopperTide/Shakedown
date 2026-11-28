// A world's group: one the project already has, none, or a new one typed in.
import { useState } from 'react';
import { Hint } from '@/components/ui/hint';
import { Input, NativeSelect } from '@/components/ui/input';

/** The choice that opens the new group's name; a blank name, which no group can have. */
const NEW = ' ';

export default function GroupField({ value, groups, onChange }: { value?: string; groups: string[]; onChange: (group: string | undefined) => void }) {
  const [adding, setAdding] = useState(false);
  const known = value && !groups.includes(value) ? [...groups, value] : groups;
  const choose = (v: string) => {
    setAdding(v === NEW);
    if (v !== NEW) onChange(v || undefined);
  };
  return (
    <div className="grid gap-3 md:grid-cols-2">
      <label className="grid gap-1 text-sm">
        <span className="text-muted">Group</span>
        <NativeSelect aria-label="Group" value={adding ? NEW : value ?? ''} onChange={(e) => choose(e.target.value)}>
          <option value="">No group</option>
          {known.map((g) => <option key={g} value={g}>{g}</option>)}
          <option value={NEW}>New group…</option>
        </NativeSelect>
      </label>
      {adding && (
        <label className="grid gap-1 text-sm">
          <span className="text-muted"><Hint hint="Often the customer this world is for.">New group name</Hint></span>
          {/* Spaces at either end are trimmed on leaving the box: a group name can't hold them. */}
          <Input aria-label="New group name" value={value ?? ''} placeholder="Customer name"
            onChange={(e) => onChange(e.target.value.trim() ? e.target.value : undefined)}
            onBlur={(e) => onChange(e.target.value.trim() || undefined)} />
        </label>
      )}
    </div>
  );
}
