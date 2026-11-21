// How to use Shakedown from Claude Code, Codex or any MCP client: the project folder comes from
// SHAKEDOWN_PROJECT, so no path is shown.
import { useState } from 'react';
import { Card } from '@/components/ui/card';
import { Segmented } from '@/components/ui/segmented';
import { SectionHead } from '@/features/editors/fields';

const COMMANDS = {
  claude: 'claude mcp add shakedown -- shaked mcp',
  codex: '[mcp_servers.shakedown]\ncommand = "shaked"\nargs = ["mcp"]',
} as const;

export default function UseFromAgent() {
  const [tool, setTool] = useState<keyof typeof COMMANDS>('claude');
  return (
    <>
      <SectionHead title="Use from an agent" about="Everything you can do here is also a tool of the Shakedown MCP server." />
      <Card>
        <div className="grid gap-3 p-[18px]">
          <Segmented label="Agent" value={tool} onChange={setTool} options={[{ id: 'claude', label: 'Claude Code' }, { id: 'codex', label: 'Codex' }]} />
          <pre className="rounded-md border border-line bg-sunken px-3 py-2 font-mono text-xs">{COMMANDS[tool]}</pre>
          <p className="text-xs text-faint">Run it where SHAKEDOWN_PROJECT names this project. Over HTTP, agents elsewhere use POST /mcp with an API token.</p>
        </div>
      </Card>
    </>
  );
}
