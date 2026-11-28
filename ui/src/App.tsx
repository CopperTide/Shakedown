import { Suspense, lazy, useCallback, useEffect, useState } from 'react';
import { Navigate, Route, Routes, useLocation, useParams } from 'react-router-dom';
import CommandPalette, { usePaletteShortcut } from './app/CommandPalette';
import { type Mode, PALETTE_EVENT } from './app/palette';
import AskDrawer from './features/assistant/AskDrawer';
import OnboardingGate from './features/onboarding/gate';
import AppShell from './features/shell/AppShell';
import { PageBody, PageHeader } from './components/ui/page';
import { EmptyState } from './components/ui/state';
import { Toaster } from './components/ui/toaster';
import { TooltipProvider } from './components/ui/tooltip';

// Pages other than Experiments load on first visit (React Flow and uPlot stay out of the first load).
const ExperimentPage = lazy(() => import('@/features/experiments/ExperimentPage'));
const ExperimentsCompare = lazy(() => import('@/features/experiments/ExperimentPage').then((m) => ({ default: m.ComparePage })));
const NewExperimentPage = lazy(() => import('@/features/experiments/NewExperimentPage'));
const Home = lazy(() => import('@/features/start/StartPage'));
const HomePage = lazy(() => import('@/features/home/HomePage'));
const ExperimentsListPage = lazy(() => import('@/features/experiments/ExperimentsListPage'));
const FileExperimentPage = lazy(() => import('@/features/experiments/FileExperimentPage'));
const ItemListPage = lazy(() => import('@/features/editors/ItemListPage'));
const WorldEditor = lazy(() => import('@/features/editors/world/WorldEditor'));
const ScenarioEditor = lazy(() => import('@/features/editors/scenario/ScenarioEditor'));
const SetupEditor = lazy(() => import('@/features/editors/setup/SetupEditor'));
const NewFileExperiment = lazy(() => import('@/features/editors/experiment/NewExperimentPage'));
const RuntimesPage = lazy(() => import('@/features/runtimes/RuntimesPage'));
const ProjectSettingsPage = lazy(() => import('@/features/settings/ProjectSettingsPage'));
const GetStartedPage = lazy(() => import('@/features/onboarding/GetStartedPage'));
const ProcessPage = lazy(() => import('@/features/process/ProcessPage'));
const Compare = lazy(() => import('@/features/compare/ComparePage'));
const CompareMany = lazy(() => import('@/features/compare/CompareManyPage'));
const ConnectPage = lazy(() => import('@/features/connect/ConnectPage'));
const BuildPage = lazy(() => import('@/features/build/BuildPage'));
const Run = lazy(() => import('@/features/run/RunPage'));
const Runs = lazy(() => import('@/features/runs/RunsPage'));
const Team = lazy(() => import('@/features/team/TeamPage'));
const World = lazy(() => import('@/features/world/WorldPage'));
const SettingsModels = lazy(() => import('@/features/settings/ModelsPage'));
const SettingsTokens = lazy(() => import('@/features/settings/TokensPage'));
const SettingsObservability = lazy(() => import('@/features/settings/ObservabilityPage'));
const SettingsAdvanced = lazy(() => import('@/features/settings/AdvancedPage'));

function NotFound() {
  return (
    <>
      <PageHeader title="Not found" />
      <PageBody><EmptyState title="There is no page at this address." hint="Press ⌘K to go to an experiment or a run." /></PageBody>
    </>
  );
}

/** An old address (suites became experiments), sent on to its new one. */
function Moved({ to }: { to: (p: Record<string, string | undefined>) => string }) {
  return <Navigate to={to(useParams())} replace />;
}

/** Old addresses (history, suites, analytics), sent on to their new ones. */
const MOVED = [
  <Route key="history" path="/history" element={<Navigate to="/runs" replace />} />,
  <Route key="suites" path="/suites" element={<Navigate to="/" replace />} />,
  <Route key="suites-compare" path="/suites/compare/:a/:b" element={<Moved to={(p) => `/experiments/compare?ids=${p.a},${p.b}`} />} />,
  <Route key="suite" path="/suites/:id" element={<Moved to={(p) => `/experiments/${p.id}`} />} />,
  <Route key="analytics" path="/analytics" element={<Navigate to="/experiments" replace />} />,
];

function AppRoutes() {
  return (
    <Suspense fallback={null}>
      <Routes>
        <Route path="/" element={<HomePage />} />
        <Route path="/experiments" element={<ExperimentsListPage />} />
        <Route path="/worlds" element={<ItemListPage kind="worlds" />} />
        <Route path="/worlds/:id" element={<WorldEditor />} />
        <Route path="/scenarios" element={<ItemListPage kind="scenarios" />} />
        <Route path="/scenarios/:id" element={<ScenarioEditor />} />
        <Route path="/setups" element={<ItemListPage kind="setups" />} />
        <Route path="/setups/:id" element={<SetupEditor />} />
        <Route path="/runtimes" element={<RuntimesPage />} />
        <Route path="/settings/project" element={<ProjectSettingsPage />} />
        <Route path="/experiments/new" element={<NewFileExperiment />} />
        <Route path="/experiments/new/direct" element={<NewExperimentPage />} />
        <Route path="/experiments/compare" element={<ExperimentsCompare />} />
        <Route path="/experiments/file/:id" element={<FileExperimentPage />} />
        <Route path="/experiments/:id" element={<ExperimentPage />} />
        <Route path="/run" element={<Home />} />
        <Route path="/runs" element={<Runs />} />
        <Route path="/runs/:runId/:branchId" element={<Run />} />
        <Route path="/team" element={<Team />} />
        <Route path="/connect" element={<ConnectPage />} />
        <Route path="/worlds/new" element={<BuildPage />} />
        <Route path="/compare" element={<CompareMany />} />
        <Route path="/compare/:a/:b" element={<Compare />} />
        <Route path="/control/:runId/:branchId" element={<ProcessPage />} />
        <Route path="/world/:runId/:branchId" element={<World />} />
        <Route path="/settings" element={<Navigate to="/settings/models" replace />} />
        <Route path="/settings/models" element={<SettingsModels />} />
        <Route path="/settings/tokens" element={<SettingsTokens />} />
        <Route path="/settings/observability" element={<SettingsObservability />} />
        <Route path="/settings/advanced" element={<SettingsAdvanced />} />
        {MOVED}
        <Route path="*" element={<NotFound />} />
      </Routes>
    </Suspense>
  );
}

export default function App() {
  const bare = useLocation().pathname.startsWith('/get-started');
  const [palette, setPalette] = useState(false);
  const [start, setStart] = useState<Mode | undefined>(undefined);
  const toggle = useCallback(() => { setStart(undefined); setPalette((o) => !o); }, []);
  usePaletteShortcut(toggle);
  useEffect(() => {
    const open = (e: Event) => { setStart((e as CustomEvent<Mode>).detail); setPalette(true); };
    window.addEventListener(PALETTE_EVENT, open);
    return () => window.removeEventListener(PALETTE_EVENT, open);
  }, []);
  return (
    <TooltipProvider delayDuration={300}>
      <OnboardingGate>
        {bare ? (
          <><Suspense fallback={null}><GetStartedPage /></Suspense><AskDrawer /></>
        ) : (
          <AppShell onPalette={(m) => { setStart(m); setPalette(true); }}>
            <AppRoutes />
          </AppShell>
        )}
      </OnboardingGate>
      <CommandPalette open={palette} onOpenChange={setPalette} start={start} />
      <Toaster />
    </TooltipProvider>
  );
}
