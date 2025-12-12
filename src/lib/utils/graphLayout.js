import dagre from 'dagre';

/**
 * Configura e calcula layout de grafo usando dagre
 * @param {Array} panels - Array de painéis com {id, entityId, relationships}
 * @param {Object} options - Opções de layout
 * @returns {Map} Map de panelId -> {x, y}
 */
export function calculateGraphLayout(panels, options = {}) {
	const {
		rankdir = 'TB', // TB (top-bottom), LR (left-right), BT, RL
		nodesep = 100, // Separação horizontal entre nós
		ranksep = 150, // Separação vertical entre níveis
		nodeWidth = 400, // Largura do painel
		nodeHeight = 300 // Altura do painel
	} = options;

	// Criar grafo dagre
	const g = new dagre.graphlib.Graph();

	// Configurar opções do grafo
	g.setGraph({
		rankdir,
		nodesep,
		ranksep,
		marginx: 50,
		marginy: 50
	});

	// Default para edges
	g.setDefaultEdgeLabel(() => ({}));

	// Adicionar nós com dimensões individuais
	panels.forEach(panel => {
		// Usar dimensões específicas do painel, ou fallback para defaults
		const width = panel.width || nodeWidth;
		const height = panel.height || nodeHeight;

		g.setNode(panel.id.toString(), {
			width: width,
			height: height,
			label: panel.entityLabel || panel.entityId
		});
	});

	// Adicionar edges baseadas em relacionamentos
	panels.forEach(panel => {
		if (panel.relationships && Array.isArray(panel.relationships)) {
			panel.relationships.forEach(targetEntityId => {
				// Encontrar painel target
				const targetPanel = panels.find(p => p.entityId === targetEntityId);
				if (targetPanel) {
					g.setEdge(panel.id.toString(), targetPanel.id.toString());
				}
			});
		}
	});

	// Calcular layout
	dagre.layout(g);

	// Extrair posições - dagre já considera as dimensões individuais e nodesep
	const positions = new Map();
	g.nodes().forEach(nodeId => {
		const node = g.node(nodeId);
		// dagre usa centro, converter para top-left usando dimensões do próprio nó
		positions.set(parseInt(nodeId), {
			x: node.x - node.width / 2,
			y: node.y - node.height / 2
		});
	});

	return positions;
}

/**
 * Extrai relacionamentos de uma entidade para construir o grafo
 * @param {Object} entityData - Dados da entidade (JSON parseado)
 * @returns {Array<string>} Array de IRIs de entidades relacionadas
 */
export function extractRelationships(entityData) {
	if (!entityData || !entityData.properties) {
		return [];
	}

	const relationships = [];

	// Percorrer todas as propriedades (é um array de objetos)
	if (Array.isArray(entityData.properties)) {
		entityData.properties.forEach(prop => {
			// Verificar se é ObjectProperty e se tem valor
			if (prop.isObjectProperty && prop.value) {
				// O valor pode ser string (IRI) ou objeto com iri
				if (typeof prop.value === 'string') {
					relationships.push(prop.value);
				} else if (prop.value.iri) {
					relationships.push(prop.value.iri);
				}
			}
		});
	}

	return relationships;
}

/**
 * Calcula bounding box de todos os painéis
 * @param {Map} positions - Map de posições
 * @param {number} nodeWidth - Largura do nó
 * @param {number} nodeHeight - Altura do nó
 * @returns {Object} {minX, minY, maxX, maxY, width, height, centerX, centerY}
 */
export function calculateBoundingBox(positions, nodeWidth = 400, nodeHeight = 300) {
	let minX = Infinity;
	let minY = Infinity;
	let maxX = -Infinity;
	let maxY = -Infinity;

	positions.forEach(pos => {
		minX = Math.min(minX, pos.x);
		minY = Math.min(minY, pos.y);
		maxX = Math.max(maxX, pos.x + nodeWidth);
		maxY = Math.max(maxY, pos.y + nodeHeight);
	});

	return {
		minX,
		minY,
		maxX,
		maxY,
		width: maxX - minX,
		height: maxY - minY,
		centerX: (minX + maxX) / 2,
		centerY: (minY + maxY) / 2
	};
}
