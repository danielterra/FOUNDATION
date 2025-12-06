<script>
	import { getIconType, getDatatypeIcon } from '$lib/utils/formatters';

	export let groupedProperties = { own: [], inheritedByClass: {} };
	export let applicableProperties = [];
	export let expandedSections = {};
	export let toggleSection;
	export let onNavigateToNode;
	export let currentNodeLabel = '';
	export let currentNodeIcon = null;
</script>

{#if applicableProperties.length > 0}
	<section class="panel-section">
		<button class="section-header" on:click={() => toggleSection('properties')}>
			<span class="section-icon">{expandedSections.properties ? '▼' : '▶'}</span>
			<span class="section-title">Properties</span>
		</button>
		{#if expandedSections.properties}
			<div class="section-content">
				<!-- Own Properties -->
				{#if groupedProperties.own.length > 0}
					<div class="inherited-group-container">
						<div class="inherited-icon-sticky" title="Own properties of {currentNodeLabel}">
							{#if currentNodeIcon}
								{#if getIconType(currentNodeIcon) === 'image'}
									<img src={currentNodeIcon} alt={currentNodeLabel} class="inherited-icon-img" />
								{:else}
									<span class="material-symbols-outlined inherited-icon-symbol">{currentNodeIcon}</span>
								{/if}
							{:else}
								<span class="inherited-icon-fallback">{currentNodeLabel}</span>
							{/if}
						</div>
						<div class="inherited-properties-list">
							{#each groupedProperties.own as prop}
								<div class="property-item">
									<div class="property-header">
										<div class="property-title-line">
											<span class="property-name">{prop.property_label}</span>
											<div class="property-badges">
												{#if prop.cardinality}
													<span
														class="cardinality-badge {prop.cardinality === 'exactly one' ? 'cardinality-single' : 'cardinality-multiple'}"
														title={prop.cardinality}
													>
														<span class="material-symbols-outlined">
															{prop.cardinality === 'exactly one' ? 'looks_one' : 'all_inclusive'}
														</span>
													</span>
												{/if}
												{#if prop.property_type === 'object'}
													<span class="type-badge type-reference" title="Reference">
														<span class="material-symbols-outlined">link</span>
													</span>
													{#if prop.range_label || prop.range}
														{#if prop.range}
															<button class="range-badge" on:click={() => onNavigateToNode(prop.range)} title={prop.range_label || prop.range.split(/[/#]/).pop()}>
																{#if prop.range_icon}
																	{#if getIconType(prop.range_icon) === 'image'}
																		<img src={prop.range_icon} alt="icon" class="range-icon-img" />
																	{:else}
																		<span class="material-symbols-outlined range-icon-symbol">{prop.range_icon}</span>
																	{/if}
																{:else}
																	{prop.range_label || prop.range.split(/[/#]/).pop()}
																{/if}
															</button>
														{:else}
															<span class="range-badge-static">{prop.range_label}</span>
														{/if}
													{/if}
												{:else}
													<span class="type-badge type-datatype" title={prop.range_label || 'String'}>
														<span class="material-symbols-outlined">{getDatatypeIcon(prop.range_label)}</span>
													</span>
												{/if}
											</div>
										</div>
										{#if prop.description}
											<div class="property-description">{prop.description}</div>
										{/if}
									</div>
								</div>
							{/each}
						</div>
					</div>
				{/if}

				<!-- Inherited Properties grouped by parent class -->
				{#each Object.entries(groupedProperties.inheritedByClass) as [classId, classGroup]}
					<div class="inherited-group-container">
						<div class="inherited-icon-sticky" title="Inherited from {classGroup.className}">
							{#if classGroup.classIcon}
								{#if getIconType(classGroup.classIcon) === 'image'}
									<img src={classGroup.classIcon} alt={classGroup.className} class="inherited-icon-img" />
								{:else}
									<span class="material-symbols-outlined inherited-icon-symbol">{classGroup.classIcon}</span>
								{/if}
							{:else}
								<span class="inherited-icon-fallback">{classGroup.className}</span>
							{/if}
						</div>
						<div class="inherited-properties-list">
							{#each classGroup.properties as prop}
								<div class="property-item">
									<div class="property-header">
										<div class="property-title-line">
											<span class="property-name">{prop.property_label}</span>
											<div class="property-badges">
												{#if prop.cardinality}
													<span
														class="cardinality-badge {prop.cardinality === 'exactly one' ? 'cardinality-single' : 'cardinality-multiple'}"
														title={prop.cardinality}
													>
														<span class="material-symbols-outlined">
															{prop.cardinality === 'exactly one' ? 'looks_one' : 'all_inclusive'}
														</span>
													</span>
												{/if}
												{#if prop.property_type === 'object'}
													<span class="type-badge type-reference" title="Reference">
														<span class="material-symbols-outlined">link</span>
													</span>
													{#if prop.range_label || prop.range}
														{#if prop.range}
															<button class="range-badge" on:click={() => onNavigateToNode(prop.range)} title={prop.range_label || prop.range.split(/[/#]/).pop()}>
																{#if prop.range_icon}
																	{#if getIconType(prop.range_icon) === 'image'}
																		<img src={prop.range_icon} alt="icon" class="range-icon-img" />
																	{:else}
																		<span class="material-symbols-outlined range-icon-symbol">{prop.range_icon}</span>
																	{/if}
																{:else}
																	{prop.range_label || prop.range.split(/[/#]/).pop()}
																{/if}
															</button>
														{:else}
															<span class="range-badge-static">{prop.range_label}</span>
														{/if}
													{/if}
												{:else}
													<span class="type-badge type-datatype" title={prop.range_label || 'String'}>
														<span class="material-symbols-outlined">{getDatatypeIcon(prop.range_label)}</span>
													</span>
												{/if}
											</div>
										</div>
										{#if prop.description}
											<div class="property-description">{prop.description}</div>
										{/if}
									</div>
								</div>
							{/each}
						</div>
					</div>
				{/each}
			</div>
		{/if}
	</section>
{/if}
