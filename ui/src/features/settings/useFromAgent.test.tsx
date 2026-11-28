import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { agentNote, UseFromAgentView } from './UseFromAgent';

describe('Use from an agent', () => {
  it('locally: the stdio command, run where SHAKEDOWN_PROJECT names the project', () => {
    const html = renderToStaticMarkup(<UseFromAgentView server={null} />);
    expect(html).toContain('claude mcp add shakedown -- shaked mcp');
    expect(html).not.toContain('SHAKEDOWN_PROJECT'); // the note shows on hovering the title
    expect(agentNote(false)).toContain('SHAKEDOWN_PROJECT');
  });
  it('hosted: the HTTP server of this page with an API token, and no local folder', () => {
    const html = renderToStaticMarkup(<UseFromAgentView server="https://app.example.com/sd/mcp" />);
    expect(html).toContain('claude mcp add --transport http shakedown https://app.example.com/sd/mcp --header &quot;Authorization: Bearer $SHAKEDOWN_TOKEN&quot;');
    expect(agentNote(true)).toContain('API tokens');
    expect(agentNote(true)).not.toContain('SHAKEDOWN_PROJECT');
    const codex = renderToStaticMarkup(<UseFromAgentView server="https://app.example.com/sd/mcp" initial="codex" />);
    expect(codex).toContain('url = &quot;https://app.example.com/sd/mcp&quot;');
    expect(codex).toContain('bearer_token_env_var = &quot;SHAKEDOWN_TOKEN&quot;');
    for (const h of [html, codex]) {
      expect(h).not.toContain('SHAKEDOWN_PROJECT');
      expect(h).not.toContain('shaked mcp');
    }
  });
});
