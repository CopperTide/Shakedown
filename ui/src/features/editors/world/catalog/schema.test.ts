import { expect, it } from 'vitest';
import { schemaOperations } from './schema';
it('shows real OpenAPI methods without treating path parameters as operations', () => {
  const operations = schemaOperations(JSON.stringify({ openapi: '3.1.0', paths: { '/orders': { parameters: [], get: { summary: 'List orders' }, post: { requestBody: { required: true } } } } }));
  expect(operations.map((o) => o.name)).toEqual(['GET /orders', 'POST /orders']);
  expect(operations[0].description).toBe('List orders');
  expect(operations[1].schema).toEqual({ requestBody: { required: true } });
});
it('shows MCP tools and input schemas from a tools/list response', () => {
  const inputSchema = { type: 'object', properties: { id: { type: 'string' } }, required: ['id'] };
  expect(schemaOperations(JSON.stringify({ jsonrpc: '2.0', result: { tools: [{ name: 'get_order', inputSchema }] } }))).toEqual([{ name: 'get_order', description: '', schema: inputSchema }]);
});
it('rejects unrelated JSON and malformed tools', () => {
  expect(() => schemaOperations('{}')).toThrow('OpenAPI');
  expect(() => schemaOperations('{')).toThrow();
  expect(() => schemaOperations('{"tools":[{}]}')).toThrow('name');
});
