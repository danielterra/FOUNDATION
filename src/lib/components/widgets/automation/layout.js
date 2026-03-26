import dagre from '@dagrejs/dagre'

const NODE_WIDTH = 220
const NODE_SIZES = {
  automation_StartEvent:      { width: NODE_WIDTH, height: 50 },
  automation_TimerStartEvent: { width: NODE_WIDTH, height: 50 },
  automation_EndEvent:        { width: NODE_WIDTH, height: 50 },
  automation_AgentTask:       { width: NODE_WIDTH, height: 55 },
  automation_CodeTask:        { width: NODE_WIDTH, height: 55 },
  automation_ScriptTask:      { width: NODE_WIDTH, height: 55 },
  automation_RequestTask:     { width: NODE_WIDTH, height: 55 },
  automation_UserTask:        { width: NODE_WIDTH, height: 68 },
  automation_Gateway:         { width: NODE_WIDTH, height: 55 },
  automation_SubProcess:      { width: NODE_WIDTH, height: 60 },
  automation_NOVAMessageTask: { width: NODE_WIDTH, height: 55 },
}

const STATUS_BADGE_HEIGHT = 30
const RANKSEP = 320
const OUTPUT_HANDLE_SPACING = 26

function subProcessExtraHeight(node) {
  return (node.data?.outputClasses?.length ?? 0) > 1
    ? (node.data.outputClasses.length - 1) * OUTPUT_HANDLE_SPACING
    : 0
}

function findBackEdges(nodes, edges) {
  const adj = new Map(nodes.map(n => [n.id, []]))
  const inDegree = new Map(nodes.map(n => [n.id, 0]))
  for (const e of edges) {
    adj.get(e.source)?.push(e.target)
    inDegree.set(e.target, (inDegree.get(e.target) ?? 0) + 1)
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
  // Start from source nodes first so DFS follows the natural flow direction.
  // This prevents forward edges from being misclassified as back-edges when
  // a downstream node (e.g. Gateway) appears before its predecessors in the array.
  for (const { id } of nodes) {
    if (inDegree.get(id) === 0 && !visited.has(id)) dfs(id)
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
    const base = NODE_SIZES[node.data?.nodeType] ?? { width: 170, height: 55 }
    const size = { ...base, height: base.height + subProcessExtraHeight(node) }
    g.setNode(node.id, { width: size.width, height: size.height + STATUS_BADGE_HEIGHT })
  }
  for (const edge of layoutEdges) {
    g.setEdge(edge.source, edge.target)
  }

  dagre.layout(g)

  return nodes.map(node => {
    const base = NODE_SIZES[node.data?.nodeType] ?? { width: 170, height: 55 }
    const size = { ...base, height: base.height + subProcessExtraHeight(node) }
    const totalHeight = size.height + STATUS_BADGE_HEIGHT
    const { x, y } = g.node(node.id)
    return { ...node, position: { x: x - size.width / 2, y: y - totalHeight / 2 } }
  })
}
