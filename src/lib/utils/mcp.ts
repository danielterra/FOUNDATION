const MCP_PORT = 47177;

interface McpToolResult {
  success: boolean;
  result: unknown;
  error: string | null;
}

export async function callMcpTool(name: string, args: Record<string, unknown>): Promise<McpToolResult> {
  const response = await fetch(`http://localhost:${MCP_PORT}/mcp`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      jsonrpc: '2.0',
      id: crypto.randomUUID(),
      method: 'tools/call',
      params: { name, arguments: args }
    })
  });

  const json = await response.json();

  if (json.error) {
    throw new Error(json.error.message ?? 'MCP error');
  }

  const text = json.result?.content?.[0]?.text;
  if (!text) throw new Error('Empty MCP response');

  return JSON.parse(text) as McpToolResult;
}
