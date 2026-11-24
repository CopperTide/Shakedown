// A read-only look at a world's data file, or at the files in a data folder.
import { useEffect, useState } from 'react';
import { worldDataFile, worldDataFiles } from '@/api/project';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent } from '@/components/ui/dialog';
import { errorMessage } from '@/lib/errors';

type Shown = Awaited<ReturnType<typeof worldDataFile>>;

const kb = (n: number) => `${Math.max(1, Math.round(n / 1024))} KB`;

function FileText({ world, file }: { world: string; file: string }) {
  const [shown, setShown] = useState<Shown>();
  const [error, setError] = useState('');
  useEffect(() => { worldDataFile(world, file).then(setShown, (e: unknown) => setError(errorMessage(e))); }, [world, file]);
  if (error) return <p role="alert" className="p-4 text-sm text-alarm">{error}</p>;
  if (!shown) return <p className="p-4 text-sm text-faint">Loading…</p>;
  if (shown.binary) return <p className="p-4 text-sm text-muted">Binary file · {kb(shown.size)}</p>;
  return (
    <>
      <pre className="min-h-0 flex-1 overflow-auto p-4 font-mono text-xs leading-5 whitespace-pre-wrap" aria-label={`Contents of ${file}`}>{shown.text}</pre>
      {shown.truncated && <p className="border-t border-line px-4 py-2 text-xs text-muted">Truncated: showing the first 1 MiB of {kb(shown.size)}.</p>}
    </>
  );
}

/** The files at `source` (the file itself, or those in the folder it names): one is shown, several are listed. */
function Files({ world, source }: { world: string; source: string }) {
  const [files, setFiles] = useState<string[]>();
  const [file, setFile] = useState<string>();
  const [error, setError] = useState('');
  useEffect(() => {
    const inside = (p: string) => p === source || p.startsWith(`${source.replace(/\/$/, '')}/`);
    worldDataFiles(world).then((r) => {
      const found = r.files.map((f) => f.path).filter(inside);
      setFiles(found);
      if (found.length === 1) setFile(found[0]);
    }, (e: unknown) => setError(errorMessage(e)));
  }, [world, source]);
  if (error) return <p role="alert" className="p-4 text-sm text-alarm">{error}</p>;
  if (file) return <FileText world={world} file={file} />;
  if (!files) return <p className="p-4 text-sm text-faint">Loading…</p>;
  if (files.length === 0) return <p className="p-4 text-sm text-muted">No files here yet.</p>;
  return (
    <ul className="min-h-0 flex-1 overflow-auto p-2">
      {files.map((f) => (
        <li key={f} className="flex items-center justify-between gap-2 px-2 py-1 font-mono text-xs">
          {f}<Button size="sm" onClick={() => setFile(f)}>View</Button>
        </li>
      ))}
    </ul>
  );
}

/** A "View" button that opens `source` (a file or folder of the world's data) read-only. */
export default function DataFileViewer({ world, source }: { world: string; source: string }) {
  const [open, setOpen] = useState(false);
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <button type="button" className="text-xs text-primary" onClick={() => setOpen(true)}>View</button>
      <DialogContent title={source} description="Read-only: a copy of the file in the world's data folder." className="top-[8vh] max-h-[84vh] w-[min(800px,calc(100vw-24px))]">
        <Files world={world} source={source} />
      </DialogContent>
    </Dialog>
  );
}
