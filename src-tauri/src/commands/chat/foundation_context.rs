use crate::eavto::Connection;
use crate::owl::{Class, Individual};

const AUTOMATION_TASK_ROOT: &str = "foundation:automation_Task";
const AUTOMATION_EVENT_ROOT: &str = "foundation:automation_Event";
const AUTOMATION_GATEWAY_ROOT: &str = "foundation:automation_Gateway";

pub fn foundation_architecture_context(conn: &Connection) -> String {
    let task_types = list_subclasses_of(conn, AUTOMATION_TASK_ROOT);
    let event_types = list_subclasses_of(conn, AUTOMATION_EVENT_ROOT);
    let gateway_types = list_subclasses_of(conn, AUTOMATION_GATEWAY_ROOT);

    let task_list = render_list(&task_types, "nenhuma task type registrada");
    let event_list = render_list(&event_types, "nenhum event type registrado");
    let gateway_list = render_list(&gateway_types, "nenhum gateway type registrado");

    format!(
        "# Foundation — arquitetura do sistema\n\
         \n\
         Foundation é a base de conhecimento pessoal do usuário. Tudo — pessoas, tarefas, \
         anotações, widgets, agentes, automações, configurações — são indivíduos de uma \
         ontologia RDF/OWL armazenada no grafo. Você opera sobre esse grafo chamando as \
         ferramentas MCP (`search`, `describe_class`, `describe_individual`, \
         `assert_individual`, `add_property_values`, `replace_property_values`, \
         `remove_property_values`, `retract_individual`, `define_class`, `define_property`, \
         `class_graph`, entre outras).\n\
         \n\
         ## Governança da ontologia (CRÍTICO)\n\
         \n\
         **Princípio fundamental**: sempre reutilizar o que existe. NUNCA gerar redundância. \
         Cada nova classe, propriedade ou indivíduo é uma dívida semântica — duplicatas \
         fragmentam o grafo e inviabilizam queries e cálculos. Em caso de dúvida: não crie, \
         procure mais.\n\
         \n\
         ### Antes de criar uma CLASSE ou PROPRIEDADE (obrigatório)\n\
         1. Chame `class_graph` na região semântica relacionada para entender a estrutura \
         existente. Ex: `class_graph(class_iri: \"foundation:Person\", max_depth: 3)`.\n\
         2. Examine nós (classes) e arestas (relacionamentos) retornados. Procure por \
         sinônimos, variações de nome, conceitos que cobrem o mesmo território.\n\
         3. Use `search(query: \"...\", type: \"class\")` com termos alternativos (pt-BR e \
         inglês) antes de concluir que não existe.\n\
         4. Se for uma classe, identifique a superclasse apropriada — prefira estender \
         subclasseando a reinventar. Só crie raiz quando não houver pai semântico adequado.\n\
         5. Se for uma propriedade equivalente/inversa de uma existente, NÃO crie: use a \
         existente, eventualmente com `inverse_label`.\n\
         6. Só chame `define_class`/`define_property` depois que os passos acima confirmarem \
         que não há equivalente.\n\
         \n\
         ### Antes de criar um INDIVÍDUO (obrigatório)\n\
         1. Identifique a classe alvo. Chame `describe_class` para ver seus campos \
         obrigatórios, propriedades aplicáveis, subclasses e restrições.\n\
         2. Use `class_graph` quando precisar de contexto (como essa classe se conecta ao \
         resto do grafo) — especialmente para decidir quais propriedades de relacionamento \
         preencher.\n\
         3. **Procure duplicatas com chave natural**: antes de criar uma Person, busque por \
         email/nome; antes de uma Company, busque por CNPJ/nome; antes de uma Task, busque \
         por label+contexto. Use `search` filtrado por `class_iri` e por propriedades-chave.\n\
         4. Se existir — use o IRI existente. Se for preciso adicionar informação nova, use \
         `add_property_values` sobre o indivíduo existente.\n\
         5. Escolha a classe mais específica que se aplica (prefira `foundation:CSVFile` a \
         `foundation:File` quando couber).\n\
         \n\
         ### Convenções de modelagem\n\
         - **Domínio da propriedade**: `rdfs:domain` vai na classe que **possui** a \
         propriedade, nunca na classe range. Ex: `foundation:hasStatus` tem domínio \
         `owl:Thing` (não `foundation:Status`); `foundation:userRole` tem domínio \
         `foundation:UserStory` (não `foundation:Persona`).\n\
         - **Relacionamentos**: sempre declare a propriedade do lado \"muitos\" apontando \
         para o lado \"um\" (filho → pai). Para navegar no sentido inverso use `inverse_label` \
         em vez de duplicar a propriedade.\n\
         - **Tipos primitivos vs referências**: prefira `owl:ObjectProperty` apontando para \
         uma classe existente (`foundation:City`, `foundation:EmailAddress`, \
         `foundation:Person`) em vez de `xsd:string` solto. Use primitivos para valores \
         escalares/booleanos/datas, identificadores/códigos, texto livre e valores monetários.\n\
         - **Propriedades numéricas**: sempre declare `qudt:unit` (`unit:Count`, \
         `unit:Meter`, `currency:BRL`, etc.).\n\
         - **Correção de dados**: quando encontrar valor armazenado errado/sujo, corrija o \
         dado no grafo. Não adicione lógica de normalização/saneamento para compensar — fonte \
         da verdade é o grafo.\n\
         \n\
         ## Ontologia\n\
         - **Classe** (`owl:Class`): define um tipo. Ex: `foundation:Task`, `foundation:Person`.\n\
         - **Subclasse / polimorfismo** (`rdfs:subClassOf`): uma classe herda propriedades e \
         comportamentos da superclasse. Ex: `foundation:CSVFile` é subclasse de \
         `foundation:File`. Ao procurar por instâncias de uma classe, considere também \
         instâncias de subclasses — e ao criar uma instância, escolha a classe mais específica \
         que se aplique.\n\
         - **Indivíduo** (instância): cada entidade concreta. IRIs seguem o padrão \
         `foundation:ClassName_{{timestamp}}`. Use `assert_individual` para criar.\n\
         - **Propriedade**: ligação tipada entre indivíduos (object property) ou entre \
         indivíduo e literal (datatype property). Declarada via `define_property`.\n\
         \n\
         ## Tipos de propriedade\n\
         Toda propriedade se comporta como um dos quatro modos abaixo. Antes de criar uma \
         nova, verifique via `describe_class`/`describe_property` se já existe algo \
         equivalente.\n\
         \n\
         1. **Valor** — guarda um literal tipado (`xsd:string`, `xsd:integer`, `xsd:decimal`, \
         `xsd:date`, `xsd:dateTime`, `xsd:boolean`, `xsd:anyURI`) ou uma referência IRI (object \
         property com `rdfs:range` apontando para outra classe). Propriedades numéricas \
         requerem `qudt:unit` (ex: `unit:Count`, `unit:Meter`, `currency:BRL`).\n\
         2. **Cálculo** — a propriedade tem `foundation:formula` com expressão que referencia \
         outras propriedades via `{{foundation:propName}}`. O valor é recomputado \
         automaticamente quando as dependências mudam. Agregações usam \
         `foundation:aggregation` com funções como \
         `SOMA({{foundation:hasItems}}.foundation:amount)` — percorre uma coleção e agrega.\n\
         3. **Query** — a propriedade tem `foundation:queryDefinition` com JSON descrevendo \
         classe alvo e filtros. O valor da propriedade é a lista de indivíduos que batem com \
         o filtro, recomputada dinamicamente.\n\
         4. **Referência** — object property que aponta para outro indivíduo via IRI. O `rdfs:range` \
         define a classe alvo. Pode ter inverso (`owl:inverseOf`) para navegar nos dois \
         sentidos sem duplicar o triplo.\n\
         \n\
         ## Automações\n\
         Uma `foundation:automation_Process` é um fluxo BPMN-like. Cada nó é um \
         `foundation:automation_FlowNode` conectado ao processo via `foundation:partOfProcess` \
         e aos vizinhos via `foundation:flowTo`. O executor despacha cada nó para o handler \
         correspondente baseado no `rdf:type`.\n\
         \n\
         ### Eventos registrados\n\
         {event_list}\n\
         \n\
         ### Task types registrados\n\
         {task_list}\n\
         \n\
         ### Gateways\n\
         {gateway_list}\n\
         \n\
         Para uma task/processo rodar, ela precisa de `foundation:scheduledAt` no futuro ou \
         `foundation:hasStatus = foundation:InProgress`. Sem isso fica pendente.\n\
         \n\
         ## Blackboard\n\
         O blackboard é o painel visual de widgets. Existe um blackboard por conversa, e \
         um blackboard padrão (`foundation:DefaultBlackboard`) usado quando o chat interno \
         não está ativo. Cada widget renderiza uma entidade do grafo usando um \
         `foundation:WidgetType`. Você adiciona widgets via \
         `add_widget_to_blackboard(entity_iri, widget_type, blackboard_iri?)` — `blackboard_iri` \
         é opcional: se omitido, usa o blackboard da conversa ativa. A lista de widgets \
         disponíveis aparece mais adiante neste prompt na seção \"AI-creatable widgets\".",
        task_list = task_list,
        event_list = event_list,
        gateway_list = gateway_list,
    )
}

fn list_subclasses_of(conn: &Connection, root_iri: &str) -> Vec<(String, String, Option<String>)> {
    let Ok(iris) = Class::get_descendant_iris(conn, root_iri) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for iri in iris {
        if iri == root_iri {
            continue;
        }
        if let Ok(Some(ind)) = Individual::get(conn, &iri) {
            let label = ind.label.clone().unwrap_or_else(|| iri.clone());
            let comment = ind.comment.clone().filter(|c| !c.trim().is_empty());
            out.push((iri, label, comment));
        }
    }
    out.sort_by(|a, b| a.1.cmp(&b.1));
    out
}

fn render_list(items: &[(String, String, Option<String>)], empty_msg: &str) -> String {
    if items.is_empty() {
        return format!("_{}_", empty_msg);
    }
    items.iter()
        .map(|(iri, label, comment)| match comment {
            Some(c) => format!("- `{}` ({}) — {}", iri, label, c),
            None => format!("- `{}` ({})", iri, label),
        })
        .collect::<Vec<_>>()
        .join("\n")
}
