import dagre from '@dagrejs/dagre'

const NODE_SIZES = {
  automation_StartEvent:   { width: 150, height: 50 },
  automation_EndEvent:     { width: 150, height: 50 },
  automation_AgentTask:    { width: 180, height: 55 },
  automation_CodeTask:     { width: 180, height: 55 },
  automation_ScriptTask:   { width: 180, height: 55 },
  automation_RequestTask:  { width: 180, height: 55 },
  automation_UserTask:     { width: 180, height: 55 },
  automation_Gateway:      { width: 80,  height: 80 },
  automation_SubProcess:        { width: 190, height: 60 },
  automation_NOVAMessageTask:   { width: 180, height: 55 },
}

const STATUS_BADGE_HEIGHT = 30
const RANKSEP = 160

function findBackEdges(nodes, edges) {
  const adj = new Map(nodes.map(n => [n.id, []]))
  for (const e of edges) {
    adj.get(e.source)?.push(e.target)
  }
  const visited = new Set()
  const inStack = new Set()
  const backEdges = new Set()
  function dfs(id) {
    visited.add(id)
    inStack.add(id)
    for (const neighbor of (adj.get(id) ?? [])) {
      if (!visited.has(neighbor)) dfs(neighbor)
      else if (inStack.has(neighbor)) backEdges.add(`${id}->${neighbor}`)
    }
    inStack.delete(id)
  }
  for (const { id } of nodes) {
    if (!visited.has(id)) dfs(id)
  }
  return backEdges
}

export function applyDagreLayout(nodes, edges, direction = 'LR') {
  const backEdges = findBackEdges(nodes, edges)
  const layoutEdges = edges.filter(e => !backEdges.has(`${e.source}->${e.target}`))

  const g = new dagre.graphlib.Graph()
  g.setGraph({ rankdir: direction, ranksep: RANKSEP, nodesep: 35, marginx: 40, marginy: 40 })
  g.setDefaultEdgeLabel(() => ({}))

  for (const node of nodes) {
    const size = NODE_SIZES[node.data?.nodeType] ?? { width: 170, height: 55 }
    g.setNode(node.id, { width: size.width, height: size.height + STATUS_BADGE_HEIGHT })
  }
  for (const edge of layoutEdges) {
    g.setEdge(edge.source, edge.target)
  }

  dagre.layout(g)

  return nodes.map(node => {
    const size = NODE_SIZES[node.data?.nodeType] ?? { width: 170, height: 55 }
    const totalHeight = size.height + STATUS_BADGE_HEIGHT
    const { x, y } = g.node(node.id)
    return { ...node, position: { x: x - size.width / 2, y: y - totalHeight / 2 } }
  })
}
