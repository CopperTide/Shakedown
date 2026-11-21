import { PageBody, PageHeader } from '@/components/ui/page';
import { EmptyState } from '@/components/ui/state';

/** A section whose editor isn't built yet: says so plainly instead of a blank page. */
export default function Soon({ title }: { title: string }) {
  return (
    <>
      <PageHeader title={title} />
      <PageBody><EmptyState title={`${title} arrives with the editors.`} hint="Until then, the project files can be edited directly or through the Shakedown MCP server." /></PageBody>
    </>
  );
}
