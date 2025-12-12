# Unified Logging System

Sistema de logging centralizado que captura logs do **frontend** (Svelte) e **backend** (Rust) em um único arquivo.

- **Frontend**: Intercepta `console.log`, `console.warn`, `console.error`, `console.info`, `console.debug`
- **Backend**: Função `log_backend()` para logging do Rust

## 📍 Localização do Arquivo de Log

```
/Users/daniel/Library/Application Support/org.w3id.foundation/application.log
```

## 🔍 Como Visualizar os Logs

### Ver todos os logs:
```bash
cat "/Users/daniel/Library/Application Support/org.w3id.foundation/application.log"
```

### Acompanhar em tempo real:
```bash
tail -f "/Users/daniel/Library/Application Support/org.w3id.foundation/application.log"
```

### Ver últimas 50 linhas:
```bash
tail -50 "/Users/daniel/Library/Application Support/org.w3id.foundation/application.log"
```

### Filtrar por origem:
```bash
# Apenas frontend
grep "\[FRONTEND\]" "/Users/daniel/Library/Application Support/org.w3id.foundation/application.log"

# Apenas backend
grep "\[BACKEND\]" "/Users/daniel/Library/Application Support/org.w3id.foundation/application.log"
```

### Buscar por palavra-chave:
```bash
grep -i "error" "/Users/daniel/Library/Application Support/org.w3id.foundation/application.log"
```

## 📝 Formato dos Logs

```
[2025-12-12 16:35:39.702] [BACKEND] [INFO] Database initialization starting...
[timestamp]              [source]  [level] [message]
```

**Source**: `FRONTEND` ou `BACKEND`
**Níveis**: `LOG`, `INFO`, `WARN`, `ERROR`, `DEBUG`

## 🧹 Limpar Logs

### Via código JavaScript:
```javascript
import { clearLogs } from '$lib/logging.js';
await clearLogs();
```

### Via Tauri command:
```javascript
import { invoke } from '@tauri-apps/api/core';
await invoke('clear_logs');
```

### Manualmente:
```bash
rm "/Users/daniel/Library/Application Support/org.w3id.foundation/application.log"
```

## 🔧 Arquitetura

### Backend (Rust)
- **Arquivo**: `src-tauri/src/commands/logging.rs`
- **Funções públicas**:
  - `log_backend(app, level, message)` - Loga do Rust (uso interno)
- **Comandos Tauri**:
  - `log_frontend(level, message)` - Recebe logs do frontend
  - `get_log_file_path_command()` - Retorna caminho do arquivo
  - `clear_logs()` - Limpa todos os logs

**Exemplo de uso no backend:**
```rust
use crate::commands;

// Em qualquer lugar do código Rust onde você tem acesso ao AppHandle:
commands::log_backend(&app_handle, "info", "Mensagem de log");
commands::log_backend(&app_handle, "error", &format!("Erro: {}", error));
```

### Frontend (JavaScript)
- **Arquivo**: `src/lib/logging.js`
- **Funções**:
  - `initializeLogging()` - Intercepta métodos do console
  - `getLogFilePath()` - Obtém caminho do arquivo de log
  - `clearLogs()` - Limpa logs

**Uso no frontend:** Automático via console.log/warn/error/info/debug

### Integração
- **Arquivo**: `src/routes/+layout.svelte`
- Logging do frontend é inicializado automaticamente no `onMount()` do layout raiz

## 💡 Como Funciona

1. O `+layout.svelte` chama `initializeLogging()` quando a app inicia
2. `logging.js` sobrescreve os métodos nativos do console (`console.log`, `console.warn`, etc)
3. Cada chamada de console:
   - Executa o método original (para mostrar no DevTools)
   - Envia para o backend Tauri via `invoke('log_frontend')`
4. O backend salva em arquivo append-only com timestamp

## ⚠️ Notas Importantes

- Logs são salvos **apenas quando rodando no Tauri** (não no navegador web)
- Arquivo de log cresce indefinidamente - limpe periodicamente
- Falhas ao salvar logs são silenciosas (não quebram a aplicação)
- Todos os console.logs do frontend são capturados automaticamente

## 🐛 Debug

Se os logs não estão sendo salvos, verifique:

1. **App está rodando no Tauri?**
   - Verifique se `window.__TAURI__` existe no DevTools

2. **Logging foi inicializado?**
   - Procure por "📝 Frontend logging initialized" no console do DevTools

3. **Diretório existe?**
   ```bash
   ls -la "/Users/daniel/Library/Application Support/org.w3id.foundation/"
   ```

4. **Permissões de escrita?**
   ```bash
   touch "/Users/daniel/Library/Application Support/org.w3id.foundation/test.txt"
   ```

## 📊 Exemplo de Saída

### Logs unificados (Frontend + Backend):
```
[2025-12-12 16:35:39.702] [BACKEND] [INFO] Database initialization starting...
[2025-12-12 16:35:39.787] [BACKEND] [INFO] Database initialized successfully
[2025-12-12 16:35:39.832] [BACKEND] [INFO] Database stats - Total triples: 45911, Active: 45911, Transactions: 45, Entities: 3278
[2025-12-12 16:35:39.832] [BACKEND] [INFO] Database initialization complete
[2025-12-12 16:35:40.261] [FRONTEND] [LOG] +page: Setup check result: true
[2025-12-12 16:35:40.305] [FRONTEND] [WARN] Aviso importante
[2025-12-12 16:35:40.312] [FRONTEND] [ERROR] Erro capturado
```

### Logs com objetos JSON:
Objetos são automaticamente convertidos para JSON com formatação bonita (indentação de 2 espaços):

```javascript
console.log('User data:', { name: 'John', age: 30, nested: { foo: 'bar' } });
```

Resultado no arquivo de log:
```
[2025-12-12 16:31:49.936] [LOG] User data: {
  "name": "John",
  "age": 30,
  "nested": {
    "foo": "bar"
  }
}
```

**Nota:** Objetos com referências circulares são convertidos usando `String(arg)` como fallback.
