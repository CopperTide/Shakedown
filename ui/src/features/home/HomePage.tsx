// Home: your information first. The week in figures, what needs attention, and the project; while
// the project has no worlds, "Start from an example" comes first.
import { Link } from 'react-router-dom';
import { runs as listRuns } from '@/api/runs';
import { Button } from '@/components/ui/button';
import { PageBody, PageHeader } from '@/components/ui/page';
import { EmptyState, ErrorState } from '@/components/ui/state';
import { useProject } from '@/hooks/useProject';
import { useQuery } from '@/hooks/useQuery';
import ExampleCard from './ExampleCard';
import { attention, greeting, weekStats } from './home';
import NeedsAttention from './NeedsAttention';
import ProjectCard from './ProjectCard';
import Stats from './Stats';

function NoProject({ message }: { message: string }) {
  return (
    <EmptyState
      title="Set up your project"
      hint={`A project holds your worlds, scenarios, agent setups and experiments. (${message})`}
      action={<Button variant="primary" asChild><Link to="/get-started">Get started</Link></Button>}
    />
  );
}

export default function HomePage() {
  const project = useProject();
  const runs = useQuery('home-runs', listRuns);
  const now = Date.now();
  const all = runs.data ?? [];
  return (
    <>
      <PageHeader title={greeting(new Date(now).getHours())} width="wide" />
      <PageBody width="wide">
        {project.error && <ErrorState error={String(project.error)} onRetry={project.reload} />}
        {project.data && !project.data.open && <NoProject message={project.data.message} />}
        {project.data?.open && !project.data.status.counts.worlds && <ExampleCard />}
        <Stats stats={weekStats(all, now)} />
        <div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_340px]">
          <NeedsAttention branches={attention(all)} />
          {project.data?.open && <ProjectCard status={project.data.status} />}
        </div>
      </PageBody>
    </>
  );
}
