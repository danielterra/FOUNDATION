import dagre from '@dagrejs/dagre'

const TASK_WIDTH = 220
const H_PADDING = 36
const V_PADDING = 20
const LINE_HEIGHT = 16   // font-size 12px * line-height 1.3
const META_ROW_H = 18
const COMPONENT_ROW_H = 14
const STATUS_BADGE_HEIGHT = 30

const USABLE_WIDTH = TASK_WIDTH - H_PADDING
const CHARS_PER_LINE = Math.floor(USABLE_WIDTH / 8)

function estimateLines(text) {
  if (!text) return 0
  return Math.ceil(text.length / CHARS_PER_LINE)
}

function estimateTaskHeight(node) {
  const labelLines = Math.max(1, estimateLines(node.data?.label ?? ''))
  const hasMetaRow = node.data?.performedBy || node.data?.executedIn
  const hasComponent = !!node.data?.rendersComponent

  let height = V_PADDING + labelLines * LINE_HEIGHT
  if (hasMetaRow) height += META_ROW_H
  if (hasComponent) height += COMPONENT_ROW_H
  return height
}

const EVENT_WIDTH = 180
const END_EVENT_WIDTH = 200
const CONDITION_WIDTH = 180

function estimateTextLines(text, usableWidth) {
  if (!text) return 1
  const charsPerLine = Math.floor(usableWidth / 8)
  return Math.max(1, Math.ceil(text.length / charsPerLine))
}

function estimateEventHeight(label, width) {
  const usable = width - H_PADDING - 22  // icon + gap
  const lines = estimateTextLines(label, usable)
  return 16 + lines * LINE_HEIGHT  // 8px top + bottom padding
}

function estimateConditionHeight(node) {
  const value = node.data?.conditionValue ?? ''
  const usable = CONDITION_WIDTH - H_PADDING - 22 - 40  // icon + operator min width
  const lines = estimateTextLines(value, usable)
  return 12 + lines * LINE_HEIGHT
}

const TASK_TYPES = new Set(['MetaSystemTask', 'MetaUserTask', 'MetaSubProcess'])

function nodeSize(node) {
  const type = node.data?.nodeType
  const label = node.data?.label ?? ''

  if (TASK_TYPES.has(type)) {
    return { width: TASK_WIDTH, height: estimateTaskHeight(node) }
  }
  if (type === 'MetaStartEvent' || type === 'MetaIntermediateEvent' || type === 'MetaBoundaryEvent') {
    return { width: EVENT_WIDTH, height: estimateEventHeight(label, EVENT_WIDTH) }
  }
  if (type === 'MetaEndEvent') {
    return { width: END_EVENT_WIDTH, height: estimateEventHeight(label, END_EVENT_WIDTH) }
  }
  if (type === 'MetaGatewayCondition') {
    return { width: CONDITION_WIDTH, height: estimateConditionHeight(node) }
  }
  if (type === 'MetaExclusiveGateway' || type === 'MetaInclusiveGateway') {
    return { width: 160, height: estimateEventHeight(label, 160) }
  }
  if (type === 'MetaParallelGateway') return { width: 44, height: 44 }
  if (type === 'MetaEventBasedGateway') return { width: 90, height: 90 }
  return { width: 180, height: 55 }
}

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

  const sizes = new Map(nodes.map(n => [n.id, nodeSize(n)]))

  const g = new dagre.graphlib.Graph()
  g.setGraph({ rankdir: direction, ranksep: 80, nodesep: 70, marginx: 40, marginy: 40 })
  g.setDefaultEdgeLabel(() => ({}))

  for (const node of nodes) {
    const size = sizes.get(node.id)
    g.setNode(node.id, { width: size.width, height: size.height + STATUS_BADGE_HEIGHT })
  }
  for (const edge of layoutEdges) {
    g.setEdge(edge.source, edge.target)
  }

  dagre.layout(g)

  return nodes.map(node => {
    const size = sizes.get(node.id)
    const totalHeight = size.height + STATUS_BADGE_HEIGHT
    const { x, y } = g.node(node.id)
    return { ...node, position: { x: x - size.width / 2, y: y - totalHeight / 2 } }
  })
}
