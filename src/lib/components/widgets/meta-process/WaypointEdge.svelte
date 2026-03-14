<script>
  import { BaseEdge } from '@xyflow/svelte'

  let { sourceX, sourceY, targetX, targetY, data, label, labelStyle, markerEnd, style } = $props()

  const CORNER_RADIUS = 10
  // If waypoints deviate more than this from the source/target y-midpoint,
  // dagre inserted a far-away dummy node — replace with a constrained arc.
  const MAX_DETOUR = 80

  function polyline(pts) {
    if (pts.length < 2) return ''
    let d = `M${pts[0].x},${pts[0].y}`
    for (let i = 1; i < pts.length - 1; i++) {
      const prev = pts[i - 1], curr = pts[i], next = pts[i + 1]
      const dx1 = curr.x - prev.x, dy1 = curr.y - prev.y
      const dx2 = next.x - curr.x, dy2 = next.y - curr.y
      const l1 = Math.sqrt(dx1*dx1 + dy1*dy1), l2 = Math.sqrt(dx2*dx2 + dy2*dy2)
      if (l1 === 0 || l2 === 0) { d += ` L${curr.x},${curr.y}`; continue }
      const r = Math.min(CORNER_RADIUS, l1/2, l2/2)
      d += ` L${curr.x-(dx1/l1)*r},${curr.y-(dy1/l1)*r} Q${curr.x},${curr.y} ${curr.x+(dx2/l2)*r},${curr.y+(dy2/l2)*r}`
    }
    return d + ` L${pts[pts.length-1].x},${pts[pts.length-1].y}`
  }

  function arcPath(sx, sy, tx, ty, goBelow) {
    // Route the edge with a smooth arc passing above or below, proportional to horizontal distance
    const offset = Math.min(MAX_DETOUR, Math.abs(tx - sx) * 0.18 + 40)
    const arcY = goBelow ? Math.max(sy, ty) + offset : Math.min(sy, ty) - offset
    const cx = (sx + tx) / 2
    return `M${sx},${sy} C${sx+40},${sy} ${cx},${arcY} ${cx},${arcY} C${cx},${arcY} ${tx-40},${ty} ${tx},${ty}`
  }

  const edgePath = $derived(() => {
    const waypoints = data?.waypoints ?? []
    if (waypoints.length === 0) {
      const cx = (sourceX + targetX) / 2
      return `M${sourceX},${sourceY} C${cx},${sourceY} ${cx},${targetY} ${targetX},${targetY}`
    }

    const midY = (sourceY + targetY) / 2
    const spread = Math.abs(targetY - sourceY) / 2
    const maxDev = Math.max(...waypoints.map(p => Math.abs(p.y - midY)))

    if (maxDev > spread + MAX_DETOUR) {
      // Large-detour skip-rank edge — use a clean constrained arc
      const goBelow = waypoints[Math.floor(waypoints.length / 2)].y > midY
      return arcPath(sourceX, sourceY, targetX, targetY, goBelow)
    }

    return polyline([{ x: sourceX, y: sourceY }, ...waypoints, { x: targetX, y: targetY }])
  })

  const labelX = $derived((sourceX + targetX) / 2)
  const labelY = $derived((sourceY + targetY) / 2)
</script>

<BaseEdge path={edgePath()} {label} {labelStyle} {markerEnd} {style} {labelX} {labelY} />
