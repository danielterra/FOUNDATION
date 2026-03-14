import dagre from '@dagrejs/dagre'

const NODE_SIZES = {
  MetaStartEvent:        { width: 150, height: 50 },
  MetaEndEvent:          { width: 150, height: 50 },
  MetaIntermediateEvent: { width: 150, height: 50 },
  MetaSystemTask:        { width: 180, height: 55 },
  MetaUserTask:          { width: 180, height: 55 },
  MetaSubProcess:        { width: 190, height: 60 },
  MetaExclusiveGateway:  { width: 120, height: 80 },
  MetaParallelGateway:   { width: 140, height: 80 },
  MetaEventBasedGateway: { width: 90,  height: 90 },
}

const STATUS_BADGE_HEIGHT = 30

export function applyDagreLayout(nodes, edges, direction = 'LR') {
  const g = new dagre.graphlib.Graph()
  g.setGraph({ rankdir: direction, ranksep: 80, nodesep: 40, marginx: 40, marginy: 40 })
  g.setDefaultEdgeLabel(() => ({}))

  for (const node of nodes) {
    const size = NODE_SIZES[node.data?.nodeType] ?? { width: 170, height: 55 }
    g.setNode(node.id, { width: size.width, height: size.height + STATUS_BADGE_HEIGHT })
  }
  for (const edge of edges) {
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
