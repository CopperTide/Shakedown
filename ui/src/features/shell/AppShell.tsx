// The app's frame: the sidebar on the left, the top bar and the page on the right.
import type { ReactNode } from 'react';
import type { Mode } from '@/app/palette';
import StatusBar from '@/app/StatusBar';
import AskDrawer from '@/features/assistant/AskDrawer';
import Sidebar from './Sidebar';
import TopBar from './TopBar';

export default function AppShell({ onPalette, children }: { onPalette: (start?: Mode) => void; children: ReactNode }) {
  return (
    <div className="flex min-h-full">
      <div className="hidden md:block"><Sidebar /></div>
      <div className="flex min-w-0 flex-1 flex-col">
        <TopBar onPalette={onPalette} />
        <div className="min-w-0 flex-1">{children}</div>
        <StatusBar />
      </div>
      <AskDrawer />
    </div>
  );
}
