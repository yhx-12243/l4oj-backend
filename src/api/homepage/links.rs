pub const fn friend_links(locale: Option<&str>) -> [(&'static str, &'static str); 17] {
    [
        ("OI Wiki", "https://oi.wiki"),
        ("Universal Online Judge", "https://uoj.ac"),
        ("LibreOJ", "https://loj.ac"),
        ("Luogu", "https://www.luogu.com.cn"),
        ("QOJ", "https://qoj.ac"),
        ("PJudge", "https://pjudge.ac"),
        ("HydroOJ", "https://hydro.ac"),
        ("Vijos", "https://vijos.org"),
        ("OIerDb", "https://oier.baoshuo.dev"),
        ("The Lean Language Reference", "https://lean-lang.org/doc/reference/latest/"),
        ("Mathlib4 Documentation", "https://leanprover-community.github.io/mathlib4_docs/"),
        ("FPiL4", "https://lean-lang.org/functional_programming_in_lean/"),
        ("TPiL4", "https://lean-lang.org/theorem_proving_in_lean4/"),
        (
            match locale {
                Some("en_US") => "Banana Space",
                Some("ja_JP") => "バナナスペース",
                _ => "香蕉空间",
            },
            "https://www.bananaspace.org/",
        ),
        ("Art of Problem Solving", "https://artofproblemsolving.com/"),
        ("Lean4 Zulip Server", "https://leanprover.zulipchat.com/"),
        (
            match locale {
                Some("en_US") => "Lean Online Judge by Ansar",
                Some("ja_JP") => "AnsarのLeanオンライン評価",
                _ => "Ansar的Lean Online Judge",
            },
            "https://leanoj.org/",
        ),
    ]
}
