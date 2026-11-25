export interface ApiOperation { name: string; description: string; schema: unknown }
const record = (v: unknown): Record<string, unknown> => v && typeof v === 'object' && !Array.isArray(v) ? v as Record<string, unknown> : {};
/** Reads an OpenAPI JSON document or an MCP tools/list response without executing any operation. */
export function schemaOperations(text: string): ApiOperation[] {
  const doc = record(JSON.parse(text));
  const tools = doc.tools ?? record(doc.result).tools;
  if (Array.isArray(tools)) return tools.map((v) => {
    const t = record(v);
    if (typeof t.name !== 'string') throw new Error('Each MCP tool must have a name.');
    return { name: t.name, description: String(t.description ?? ''), schema: t.inputSchema ?? {} };
  });
  if (!doc.openapi && !doc.swagger) throw new Error('Choose an OpenAPI JSON document or an MCP tools/list JSON response.');
  return Object.entries(record(doc.paths)).flatMap(([path, value]) => Object.entries(record(value))
    .filter(([method]) => ['get', 'post', 'put', 'patch', 'delete', 'head', 'options', 'trace'].includes(method))
    .map(([method, operation]) => ({ name: `${method.toUpperCase()} ${path}`, description: String(record(operation).summary ?? record(operation).description ?? ''), schema: operation })));
}
