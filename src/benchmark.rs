//! Dependency-free, reproducible authored-source token estimates.

struct Case {
    name: &'static str,
    aml: &'static str,
    python: &'static str,
    javascript: &'static str,
    ruby: &'static str,
    go: &'static str,
}

const CASES: &[Case] = &[
    Case {
        name: "validated_greeting",
        aml: include_str!("../aml/benchmark_cases/validated_greeting.aml"),
        python: include_str!("../aml/benchmark_cases/validated_greeting.py"),
        javascript: include_str!("../aml/benchmark_cases/validated_greeting.js"),
        ruby: include_str!("../aml/benchmark_cases/validated_greeting.rb"),
        go: include_str!("../aml/benchmark_cases/validated_greeting.go"),
    },
    Case {
        name: "bounded_score",
        aml: include_str!("../aml/benchmark_cases/bounded_score.aml"),
        python: include_str!("../aml/benchmark_cases/bounded_score.py"),
        javascript: include_str!("../aml/benchmark_cases/bounded_score.js"),
        ruby: include_str!("../aml/benchmark_cases/bounded_score.rb"),
        go: include_str!("../aml/benchmark_cases/bounded_score.go"),
    },
    Case {
        name: "safe_percentage",
        aml: include_str!("../aml/benchmark_cases/safe_percentage.aml"),
        python: include_str!("../aml/benchmark_cases/safe_percentage.py"),
        javascript: include_str!("../aml/benchmark_cases/safe_percentage.js"),
        ruby: include_str!("../aml/benchmark_cases/safe_percentage.rb"),
        go: include_str!("../aml/benchmark_cases/safe_percentage.go"),
    },
];

const LANGUAGES: &[&str] = &["python", "javascript", "ruby", "go"];

fn comparison(case: &Case, language: &str) -> &'static str {
    match language {
        "python" => case.python,
        "javascript" => case.javascript,
        "ruby" => case.ruby,
        "go" => case.go,
        _ => unreachable!(),
    }
}

/// Mirrors the documented lexical approximation: identifier/number runs and
/// individual punctuation, with every four characters rounded up to a token.
pub fn estimate_tokens(source: &str) -> usize {
    let mut total = 0;
    let mut segment = 0;
    let flush = |length: &mut usize, count: &mut usize| {
        if *length > 0 {
            *count += (*length).div_ceil(4);
            *length = 0;
        }
    };
    for character in source.chars() {
        if character.is_ascii_alphanumeric() || character == '_' || character == '.' && segment > 0
        {
            segment += 1;
        } else {
            flush(&mut segment, &mut total);
            if !character.is_whitespace() {
                total += 1;
            }
        }
    }
    flush(&mut segment, &mut total);
    total
}

fn selected(language: &str) -> Result<Vec<&'static str>, String> {
    if language == "all" {
        Ok(LANGUAGES.to_vec())
    } else if LANGUAGES.contains(&language) {
        Ok(vec![LANGUAGES
            .iter()
            .copied()
            .find(|item| *item == language)
            .unwrap()])
    } else {
        Err(format!("unknown benchmark language: {language}"))
    }
}

pub fn report(language: &str, json: bool) -> Result<String, String> {
    let languages = selected(language)?;
    if json {
        let mut comparisons = Vec::new();
        for language in languages {
            let mut cases = Vec::new();
            let mut aml_total = 0;
            let mut comparison_total = 0;
            for case in CASES {
                let aml = estimate_tokens(case.aml);
                let other = estimate_tokens(comparison(case, language));
                aml_total += aml;
                comparison_total += other;
                let reduction = 100.0 * (other as f64 - aml as f64) / other as f64;
                cases.push(format!(
                    "{{\"case\":\"{}\",\"aml_tokens\":{},\"comparison_tokens\":{},\"source_reduction_percent\":{:.1}}}",
                    case.name, aml, other, reduction
                ));
            }
            let reduction =
                100.0 * (comparison_total as f64 - aml_total as f64) / comparison_total as f64;
            comparisons.push(format!(
                "\"{}\":{{\"cases\":[{}],\"totals\":{{\"aml_tokens\":{},\"comparison_tokens\":{},\"tokens_saved\":{},\"source_reduction_percent\":{:.1}}}}}",
                language,
                cases.join(","),
                aml_total,
                comparison_total,
                comparison_total as isize - aml_total as isize,
                reduction
            ));
        }
        return Ok(format!(
            "{{\"tokenizer\":\"lexical_estimate\",\"scope\":\"authored source only\",\"comparisons\":{{{}}}}}\n",
            comparisons.join(",")
        ));
    }

    let mut output = String::from(
        "AML source benchmark\nTokenizer: lexical_estimate\nScope: authored source only\n",
    );
    for language in languages {
        output.push_str(&format!("\nAML vs {}\n", language));
        output.push_str("case                         AML comparison  reduction\n");
        let mut aml_total = 0;
        let mut comparison_total = 0;
        for case in CASES {
            let aml = estimate_tokens(case.aml);
            let other = estimate_tokens(comparison(case, language));
            aml_total += aml;
            comparison_total += other;
            output.push_str(&format!(
                "{:<26} {:>4} {:>10} {:>9.1}%\n",
                case.name,
                aml,
                other,
                100.0 * (other as f64 - aml as f64) / other as f64
            ));
        }
        output.push_str(&format!(
            "{:<26} {:>4} {:>10} {:>9.1}%\n",
            "TOTAL",
            aml_total,
            comparison_total,
            100.0 * (comparison_total as f64 - aml_total as f64) / comparison_total as f64
        ));
    }
    output.push_str(
        "\nEstimate only. This measures authored source, not prompts, reasoning, repairs, or runtime.\n",
    );
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_is_valid_json_and_shows_savings() {
        let output = report("javascript", true).unwrap();
        let parsed = crate::json::parse(&output).unwrap();
        assert!(parsed.object().unwrap().contains_key("comparisons"));
        assert!(output.contains("\"tokens_saved\":"));
    }

    #[test]
    fn all_reference_pairs_are_smaller_in_aml() {
        for language in LANGUAGES {
            for case in CASES {
                assert!(estimate_tokens(case.aml) < estimate_tokens(comparison(case, language)));
            }
        }
    }

    #[test]
    fn rejects_unknown_language() {
        assert!(report("brainfuck", false).is_err());
    }
}
