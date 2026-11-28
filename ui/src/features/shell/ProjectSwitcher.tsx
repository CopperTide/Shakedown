import { Check, ChevronDown, Plus } from 'lucide-react';
import { useState } from 'react';
import { basePath } from '@/api/core';
import { createProject, fetchProjects, selectProject } from '@/api/projects';
import { Button } from '@/components/ui/button';
import { Dialog, DialogActions, DialogContent } from '@/components/ui/dialog';
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from '@/components/ui/dropdown-menu';
import { Input } from '@/components/ui/input';
import { useQuery } from '@/hooks/useQuery';

/** Reload at the list so editors and cached data cannot carry across projects. */
export function openSelectedProject() { window.location.assign(`${basePath(import.meta.env.BASE_URL)}/experiments`); }

export function ProjectSwitcher({ name }: { name: string }) {
  const { data, error } = useQuery('project-choices', fetchProjects);
  const [creating, setCreating] = useState(false);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState('');
  const choose = async (id: string) => {
    setBusy(true); setFailure('');
    try { await selectProject(id); openSelectedProject(); }
    catch (e) { setFailure(e instanceof Error ? e.message : 'Could not switch project'); setBusy(false); }
  };
  return <div className="mb-3">
    <DropdownMenu>
      <DropdownMenuTrigger asChild><Button className="w-full justify-between" disabled={busy} aria-label={`Project: ${name}`}><span className="truncate">{name}</span><ChevronDown className="size-4 shrink-0" /></Button></DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-56">
        {data?.projects.map((p) => <DropdownMenuItem key={p.id} disabled={busy || p.id === data.current} onSelect={() => { void choose(p.id); }}><span className="flex-1 truncate">{p.name}</span>{p.id === data.current && <Check className="size-4" />}</DropdownMenuItem>)}
        <DropdownMenuItem onSelect={() => setCreating(true)}><Plus className="size-4" />New project</DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
    {(failure || error) && <p role="alert" className="mt-2 text-xs text-alarm">{failure || String(error)}</p>}
    <NewProject open={creating} onOpenChange={setCreating} />
  </div>;
}

function NewProject({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
  const [name, setName] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const create = async () => {
    if (!name.trim() || busy) return;
    setBusy(true); setError('');
    try { await createProject(name.trim()); openSelectedProject(); }
    catch (e) { setError(e instanceof Error ? e.message : 'Could not create project'); setBusy(false); }
  };
  return <Dialog open={open} onOpenChange={onOpenChange}>
    <DialogContent title="New project">
      <form onSubmit={(e) => { e.preventDefault(); void create(); }} className="p-4">
        <label htmlFor="new-project-name" className="mb-2 block text-sm">Project name</label>
        <Input id="new-project-name" value={name} onChange={(e) => setName(e.target.value)} maxLength={120} required disabled={busy} />
        {error && <p role="alert" className="mt-2 text-sm text-alarm">{error}</p>}
      </form>
      <DialogActions label={busy ? 'Creating…' : 'Create project'} disabled={busy || !name.trim()} onCancel={() => onOpenChange(false)} onGo={() => { void create(); }} />
    </DialogContent>
  </Dialog>;
}
