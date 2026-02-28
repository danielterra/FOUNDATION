# Update Models Script

Script em Rust que consulta a API da Claude e usa o **Claude CLI** para atualizar inteligentemente o arquivo `core-ontology/AIModel.ttl` com os modelos mais recentes.

## Como usar

### Pré-requisitos

1. **Arquivo `.env`** na raiz do projeto com a chave da API:
   ```bash
   ANTHROPIC_API_KEY=sk-ant-api03-...
   ```

2. **Claude CLI** instalado e configurado:
   ```bash
   # Instalar Claude CLI
   npm install -g @anthropic-ai/claude-cli

   # Ou via Homebrew (macOS)
   brew install claude
   ```

### Executar o script

```bash
# Usando npm (recomendado)
npm run update:models

# Ou diretamente com Cargo
cd scripts/update-models
cargo run --release
```

### Durante um release

O script já está integrado ao processo de release. Quando você executar:

```bash
npm run build:release
```

O script será executado automaticamente antes do build, garantindo que os modelos estejam atualizados.

## O que o script faz

1. 🔍 Busca a lista de modelos disponíveis na API da Claude
2. 📦 Cria um backup do arquivo `AIModel.ttl` atual
3. 🤖 Usa o **Claude CLI** para atualizar o arquivo de forma inteligente:
   - ✅ **PRESERVA** todas as customizações manuais (preços, comentários, propriedades extras)
   - ✅ Atualiza **APENAS** a seção "Claude Model Instances"
   - ✅ Mantém formatação e estrutura consistente
   - ✅ Adiciona novos modelos da API
   - ✅ Marca o primeiro modelo como `isDefaultModel: true`
   - ✅ Adiciona capabilities baseadas no nome do modelo
4. ✅ Salva o arquivo atualizado preservando edições manuais

## Vantagens sobre o script anterior

### ❌ Antes (script hardcoded)
- Sobrescrevia o arquivo inteiro
- Perdia customizações manuais
- Lógica de geração fixa em código

### ✅ Agora (Claude CLI)
- **Preserva customizações manuais**
- Edições cirúrgicas apenas na seção de modelos
- IA entende contexto e mantém consistência
- Flexível para mudanças futuras no formato

## Exemplo de customização preservada

Se você adicionar manualmente preços ao TTL:

```turtle
foundation:ClaudeSonnet46 a foundation:AIModel ;
    rdfs:label "Claude Sonnet 4.6" ;
    foundation:modelIdentifier "claude-sonnet-4-6" ;
    foundation:inputPricePerMTok 3.0 ;    # ✅ PRESERVADO
    foundation:outputPricePerMTok 15.0 ;  # ✅ PRESERVADO
    ...
```

O script vai **manter essas propriedades** ao atualizar!

## Backup

Antes de modificar o arquivo, o script cria automaticamente um backup em:
```
core-ontology/AIModel.ttl.backup
```

Caso algo dê errado, você pode restaurar manualmente o backup.

## Troubleshooting

### Erro: "Failed to spawn claude command"
- Certifique-se de que o Claude CLI está instalado: `which claude`
- Instale via: `npm install -g @anthropic-ai/claude-cli`

### Erro: "Claude CLI failed"
- Verifique se você tem créditos/acesso ao Claude API
- Configure o Claude CLI: `claude configure`
