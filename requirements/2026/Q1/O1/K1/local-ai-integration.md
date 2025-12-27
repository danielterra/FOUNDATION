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
<summary><strong>Solution 1: TinyLlama 1.1B (Q4_K_M)</strong> [❌ TESTED - INSUFFICIENT]</summary>

**Status:** ✅ MWE Implemented | ❌ Failed Logic Tests

**Implementation (2025-12-26):**
- ✅ Integrated llama-cpp-2 (v0.1.126)
- ✅ Model bundled: `resources/tinyllama-1.1b-chat-v1.0.Q4_K_M.gguf` (638MB)
- ✅ Module: `src-tauri/src/ai/mod.rs` with greedy sampling
- ✅ Commands: `ai__initialize()`, `ai__generate()`
- ✅ Metal acceleration (Apple M3)

**Performance:**
- Load time: 6.5s
- Simple generation (10 tokens): 165-223ms
- Complex generation (50 tokens): 320-630ms
- Memory: 44MB KV cache

**Test Results:**

| Test Type | Format | Success Rate | Notes |
|-----------|--------|--------------|-------|
| Direct Q&A | `Q: 2+2?\nA:` | ✅ 100% | Response: "4" ✓ |
| Chat Format | `<\|user\|>1+1?` | ✅ 100% | Response: "2" ✓ |
| Multiple Choice | 5 formats tested | ❌ 20% | **MAJOR ISSUE** |

**Multiple Choice Failure Analysis:**
- Tested 5 different prompt formats
- Best format (example-based): only 1/5 correct (20%)
- **Problem:** Strong bias toward letter 'D' regardless of correct answer
- Model answers with numbers instead of letters
- Cannot reliably follow "answer with letter only" instructions

**Example Failures:**
```
Q: 2+2? (A)3 (B)4 (C)5 (D)6 → Expected: B, Got: D ❌
Q: Dog legs? (A)2 (B)4 (C)6 (D)8 → Expected: B, Got: D ❌
Q: Grass color? ...C)Green... → Expected: C, Got: D ❌
```

**Research Findings (2025-12-26):**

Investigated if we're using the model incorrectly. Key discoveries:

1. **Official Benchmarks Confirm Limitations:**
   - GSM8K (Math Reasoning): **1.6%** accuracy
   - Model explicitly "not suitable for calculations"
   - No math-specific training in base model

2. **Correct Usage Verified:**
   - ✅ Using correct chat template: `<|system|>...<|user|>...<|assistant|>`
   - ✅ Proper tokenization format
   - Note: We used greedy sampling; docs recommend temperature=0.7, top_p=0.95

3. **Model Design:**
   - Optimized for: commonsense reasoning, chat, general Q&A
   - NOT designed for: multi-step reasoning, structured tasks, math
   - Can be improved via fine-tuning (TinyGSM achieves 81.5% after 12M math examples)

**Conclusion:**
❌ **NOT SUITABLE** - Our testing methodology is correct. The model's poor performance on logic tasks (20% MC accuracy) aligns with official benchmarks (1.6% GSM8K). TinyLlama was not designed for structured reasoning tasks required for ontology assistance.

**Recommendation:** Test Solution 2 (Qwen2.5-1.5B with strong math) or Solution 3 (Phi-3-mini 3.8B)

</details>

<details>
<summary><strong>Solution 2: Qwen2.5-1.5B-Instruct (Q4_K_M)</strong> [❌ TESTED - INSUFFICIENT]</summary>

**Status:** ✅ Implemented | ❌ **60% MC Accuracy** - Not good enough

**Implementation (2025-12-27):**
- Model: `qwen2.5-1.5b-instruct-q4_k_m.gguf` (1.0GB)
- Load time: 1.5s (faster than TinyLlama)
- Inference: ~300ms

**Test Results:**

| Test | TinyLlama | Qwen2.5 | Target |
|------|-----------|---------|--------|
| Multiple Choice Logic | 20% (1/5) | **60% (3/5)** | **100%** ❌ |
| Chat Format | 100% | 100% | 100% ✅ |

**Failed Tests:**
- ❌ 5+3=8 (answered B instead of C)
- ❌ Sequence 2,4,6,8,10 (answered D instead of B)

**Conclusion:**
❌ **NOT ACCEPTABLE** - While 3x better than TinyLlama, **60% accuracy on trivial logic questions is unacceptable**. Our tests are basic - we need 100% on these before moving to complex ontology tasks.

**Next:** Test Phi-3-mini 3.8B (should achieve near-100% given 82.5% GSM8K)

</details>

<details>
<summary><strong>Solution 3: Phi-4-mini-instruct (Q4_K_M)</strong> [❌ TESTED - INSUFFICIENT]</summary>

**Status:** ✅ Implemented | ❌ **80% MC Accuracy** - Better but still not 100%

**Implementation (2025-12-27):**
- Model: `phi-4-mini-instruct-q4_k_m.gguf` (2.3GB)
- Downloaded from: `bartowski/microsoft_Phi-4-mini-instruct-GGUF`
- Load time: ~3s
- Inference: ~350ms average

**Test Results:**

| Test | TinyLlama | Qwen2.5 | Phi-4-mini | Target |
|------|-----------|---------|------------|--------|
| Multiple Choice Logic | 20% (1/5) | 60% (3/5) | **80% (4/5)** | **100%** ❌ |
| Inference Speed | 165-630ms | ~300ms | ~350ms | <5s ✅ |

**Detailed Results:**
- ✅ Math: 2+2=4 (C) - PASS
- ❌ Math: 5+3=8 (C) - FAIL (answered B)
- ✅ Logic: Dog legs=4 (C) - PASS
- ✅ Knowledge: Grass=Green (C) - PASS
- ✅ Sequence: 2,4,6,8,10 (B) - PASS

**Analysis:**
- **+33% improvement** over Qwen2.5 (60% → 80%)
- **4x improvement** over TinyLlama (20% → 80%)
- Still fails on arithmetic: 5+3=8
- GSM8K benchmark: **88.6%** (highest of all tested models)
- Newest model tested (Dec 2024)

**Conclusion:**
❌ **NOT ACCEPTABLE** - Despite being the best model tested and having highest GSM8K score (88.6%), Phi-4-mini still **fails 20% of trivial logic questions**. If a model can't reliably answer "5+3=?" we cannot trust it for complex ontology assistance.

**Recommendation:** Need to either:
1. Test larger models (Phi-4 full 14B, but 9GB file)
2. Implement prompt engineering/chain-of-thought
3. Accept that small models (<3B) are fundamentally insufficient for our use case

</details>

<details>
<summary><strong>Solution 4: Phi-3-mini 3.8B (Q4_K_M)</strong> [🔜 IF MORE POWER NEEDED]</summary>

**Why This Model:**
- Microsoft's strongest compact model
- GSM8K: **82.5%** - excellent reasoning
- Best instruction following in class

**Specs:**
- Size: ~2.3GB
- Parameters: 3.8B
- Memory: ~3-4GB total
- Hardware: 8GB+ RAM recommended

**Pros:**
- ✅ Strongest reasoning of all options
- ✅ Excellent instruction following
- ✅ Well-supported by Microsoft

**Cons:**
- ⚠️ 4x larger than TinyLlama
- ⚠️ Slower startup (~10-15s)
- ⚠️ Higher memory requirements

**Download:** `https://huggingface.co/microsoft/Phi-3-mini-4k-instruct-gguf`

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
