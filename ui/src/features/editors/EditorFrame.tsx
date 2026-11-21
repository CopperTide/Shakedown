// An editor page's frame: the title, a section list, the section, and the save bar.
import type { ReactNode } from 'react';
import { PageBody, PageHeader } from '@/components/ui/page';
import { SaveBar } from '@/components/ui/save-bar';
import { SubNav, type SubNavItem } from '@/components/ui/subnav';
import type { SaveError } from './useItemEditor';

export default function EditorFrame({ title, meta, sections, active, onSelect, dirty, onSave, onDiscard, saving, error, children }: {
  title: string;
  meta?: ReactNode;
  sections: SubNavItem[];
  active: string;
  onSelect: (id: string) => void;
  dirty: boolean;
  onSave: () => void;
  onDiscard: () => void;
  saving: boolean;
  error: SaveError | null;
  children: ReactNode;
}) {
  return (
    <>
      <PageHeader title={title} meta={meta} width="wide" />
      <PageBody width="wide">
        {error && <p role="alert" className="rounded-md border border-alarm bg-alarm-soft px-3 py-2 text-sm text-alarm">{error.message}</p>}
        <div className="grid gap-8 md:grid-cols-[200px_minmax(0,1fr)]">
          <SubNav items={sections} active={active} onSelect={onSelect} />
          <section className="min-w-0">{children}</section>
        </div>
        <SaveBar dirty={dirty} summary="Unsaved changes" onSave={onSave} onDiscard={onDiscard} saving={saving} />
      </PageBody>
    </>
  );
}
