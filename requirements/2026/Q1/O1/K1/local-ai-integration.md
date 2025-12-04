## Problem 5: Local AI Integration for User Assistance

### What

We need to embed a local AI model that assists users in building, analyzing, and automating their data without requiring external services or subscriptions. The AI must be:

- **Completely embedded** - No external dependencies, API calls, or internet required
- **Zero-configuration** - Works immediately after installation, no setup needed
- **Efficient** - Runs smoothly on consumer hardware (laptop/desktop CPU)
- **Private** - All processing happens locally, no data leaves the machine
- **Helpful** - Capable of understanding user intent and assisting with FOUNDATION tasks

### Why

**Solving this is necessary for KR1** because:
- Users need guidance to understand ontology concepts and create custom classes
- Non-technical users need assistance understanding how to structure their data
- AI can translate natural language requests ("track my expenses by category") into ontology structures
- Reduces cognitive load and training time required to use FOUNDATION effectively

**Alignment with FOUNDATION principles:**
- **Principle 1 (Simplicity)**: AI acts as assistant, making complex tasks intuitive
- **Principle 2 (Decentralization)**: No dependence on external AI services
- **Principle 6 (Origin Tracking)**: AI suggestions are tracked with origin `ai:local`

### How

**Hypotheses to test:**

---

<details open>
<summary><strong>Solution 1: TinyLlama 1.1B (Q4_K_M)</strong> [🔜 TESTING FIRST]</summary>

**Specs:**
- Size: ~600MB
- Load time: ~2-3 seconds
- Memory: ~1.5GB total
- Hardware: Runs on 4GB+ RAM systems

**Integration:**
- Add `llama_cpp` crate to `src-tauri/Cargo.toml`
- Bundle GGUF in `src-tauri/resources/`
- Create `src-tauri/src/ai/mod.rs`
- Expose via Tauri commands

**Pros:**
- ✅ Smallest footprint
- ✅ Fast startup
- ✅ Works on low-end hardware

**Cons:**
- ⚠️ May struggle with complex reasoning

**Test first:** Validate integration and basic assistance capabilities before trying larger models.

</details>

<details>
<summary><strong>Solution 2: Gemma 3:4B (Q4_K_M)</strong> [🔜 IF TINYLLAMA INSUFFICIENT]</summary>

**Specs:**
- Size: ~1.7GB
- Load time: ~5-10 seconds
- Memory: ~2-3GB total
- Hardware: Requires 8GB+ RAM

**Pros:**
- ✅ Better reasoning
- ✅ Proven on similar tasks

**Cons:**
- ⚠️ 3x larger bundle
- ⚠️ Slower startup

</details>

<details>
<summary><strong>Solution 3: Phi-3-mini 3.8B (Q4_K_M)</strong> [🔜 LAST RESORT]</summary>

**Specs:**
- Size: ~2.3GB
- Memory: ~3GB total

**Pros:**
- ✅ Strong instruction following

**Cons:**
- ⚠️ Largest bundle
- ⚠️ Less community support

</details>

---

### Success Criteria

**How we'll know this problem is solved:**

1. ✅ **Embedded successfully** - GGUF model bundled in Tauri application, loads without external dependencies
2. ✅ **Zero configuration** - Works immediately after installation, no setup wizard
3. ✅ **Acceptable performance** - Inference completes in < 5 seconds on i5-class CPU with 8GB RAM
4. ✅ **Useful assistance** - AI correctly suggests ontology structures for 4/5 common use cases (finance, contacts, projects, health)
5. ✅ **User validation** - 3/5 beta users report AI assistance was "helpful" or "very helpful" during onboarding

---

### References

- [tauri-local-lm](https://github.com/dillondesilva/tauri-local-lm) - Example Tauri + llama.cpp integration
- [llama_cpp Rust crate](https://crates.io/crates/llama_cpp) - Rust bindings
- [Gemma models on HuggingFace](https://huggingface.co/models?search=gemma)
- [GGUF format guide](https://blog.mikihands.com/en/whitedec/2025/11/20/gguf-format-complete-guide-local-llm-new-standard/)
