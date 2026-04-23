/// Palavras curtas (≤3 chars) sem valor semântico para busca, cobertas em PT/EN/ES.
/// Mantidas fora do código principal para facilitar manutenção.
pub const STOPWORDS: &[&str] = &[
    // ── Português ──────────────────────────────────────────────────────────────
    // preposições e contrações
    "com", "por", "dos", "das", "nos", "nas", "num", "dum", "pra", "pro",
    // conjunções
    "mas", "nem", "que",
    // pronomes
    "ele", "ela", "seu", "sua", "lhe", "nós",
    // verbos auxiliares e cópula
    "vai", "vou", "tem", "ter", "ser", "foi", "era", "são", "dão", "hei", "diz",
    // advérbios / partículas
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

    // ── Español ────────────────────────────────────────────────────────────────
    // artículos y contracciones
    "los", "las", "del", "una", "uno",
    // preposiciones y conjunciones
    "con", "sin",
    // pronombres
    "sus", "mis", "tus", "les", "ese", "esa", "eso",
    // verbos
    "fue", "hay", "son", "han",
    // adverbios
    "así", "aún", "más", "muy", "tan", "tal",
];
