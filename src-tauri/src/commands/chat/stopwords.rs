/// Short words (≤3 chars) with no semantic value for search, covering PT/EN/ES.
/// Kept out of the main code for easier maintenance.
pub const STOPWORDS: &[&str] = &[
    // ── Portuguese ─────────────────────────────────────────────────────────────
    // prepositions and contractions
    "com", "por", "dos", "das", "nos", "nas", "num", "dum", "pra", "pro",
    // conjunctions
    "mas", "nem", "que",
    // pronouns
    "ele", "ela", "seu", "sua", "lhe", "nós",
    // auxiliary verbs and copula
    "vai", "vou", "tem", "ter", "ser", "foi", "era", "são", "dão", "hei", "diz",
    // adverbs / particles
    "sim", "ora", "bem", "aí", "eis", "uns",

    // ── English ────────────────────────────────────────────────────────────────
    // articles / determiners
    "the", "all", "any", "few", "own",
    // conjunctions / prepositions
    "and", "but", "nor", "yet", "for", "via", "per",
    // pronouns
    "she", "her", "him", "his", "its", "our",
    // auxiliary verbs
    "are", "was", "has", "had", "did", "can", "may", "let", "got", "put",
    "set", "run", "get",
    // adverbs / particles
    "not", "too", "how", "why", "who", "yes", "now", "new", "old", "etc",

    // ── Spanish ────────────────────────────────────────────────────────────────
    // articles and contractions
    "los", "las", "del", "una", "uno",
    // prepositions and conjunctions
    "con", "sin",
    // pronouns
    "sus", "mis", "tus", "les", "ese", "esa", "eso",
    // verbs
    "fue", "hay", "son", "han",
    // adverbs
    "así", "aún", "más", "muy", "tan", "tal",
];
