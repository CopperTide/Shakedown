// How to use Shakedown from Claude Code, Codex or any MCP client. Locally the server is `shaked mcp`
// and the project folder comes from SHAKEDOWN_PROJECT, so no path is shown; hosted, it is this page's
// own `/mcp` over HTTP with an API token (Settings → API tokens).
import { useState } from 'react';
import { useMe } from '@/app/AccountMenu';
import { CORE } from '@/api/core';
import { Card } from '@/components/ui/card';
import { Segmented } from '@/components/ui/segmented';

type Tool = 'claude' | 'codex';

const LOCAL: Record<Tool, string> = {
  claude: 'claude mcp add shakedown -- shaked mcp',
  codex: '[mcp_servers.shakedown]\ncommand = "shaked"\nargs = ["mcp"]',
};

const hosted = (server: string): Record<Tool, string> => ({
  claude: `claude mcp add --transport http shakedown ${server} --header "Authorization: Bearer $SHAKEDOWN_TOKEN"`,
  codex: `[mcp_servers.shakedown]\nurl = "${server}"\nbearer_token_env_var = "SHAKEDOWN_TOKEN"`,
});

/** How to use the command, on hovering the card's title. */
export const agentNote = (hosted: boolean) => 'Everything you can do here is also a tool of the Shakedown MCP server. ' + (hosted
  ? 'Set SHAKEDOWN_TOKEN to a token from Settings → API tokens. The agent works in this project, with that token\'s scope.'
  : 'Run it where SHAKEDOWN_PROJECT names this project. Over HTTP, agents elsewhere use POST /mcp with an API token.');

/** `server`: the hosted MCP server's URL, or null for a local shaked; `initial`: the agent shown first. */
export function UseFromAgentView({ server, initial = 'claude' }: { server: string | null; initial?: Tool }) {
  const [tool, setTool] = useState<Tool>(initial);
  const commands = server ? hosted(server) : LOCAL;
  return (
    <Card title="Use from an agent" hint={agentNote(!!server)}>
      <div className="grid gap-3 p-[18px]">
        <Segmented label="Agent" value={tool} onChange={setTool} options={[{ id: 'claude', label: 'Claude Code' }, { id: 'codex', label: 'Codex' }]} />
        <pre className="whitespace-pre-wrap break-all rounded-md border border-line bg-sunken px-3 py-2 font-mono text-xs">{commands[tool]}</pre>
      </div>
    </Card>
  );
}

/** The hosted MCP server's URL: this page's core, absolute. */
const hostedServer = () => new URL(`${CORE}/mcp`, window.location.origin).toString();

export default function UseFromAgent() {
  const me = useMe();
  return <UseFromAgentView server={me?.mode === 'hosted' ? hostedServer() : null} />;
}
