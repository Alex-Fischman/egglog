use crate::*;

pub mod expr {
    use super::*;

    pub fn var(name: &str) -> Expr {
        Expr::Var(DUMMY_SPAN.clone(), name.into())
    }

    pub fn int(value: i64) -> Expr {
        Expr::Lit(DUMMY_SPAN.clone(), Literal::Int(value))
    }

    pub fn call(f: &str, xs: Vec<Expr>) -> Expr {
        Expr::Call(DUMMY_SPAN.clone(), f.into(), xs)
    }
}

pub mod fact {
    use super::*;

    pub fn equals(a: Expr, b: Expr) -> Fact {
        Fact::Eq(DUMMY_SPAN.clone(), vec![a, b])
    }

    pub fn facts(facts: Vec<Fact>) -> Facts {
        GenericFacts(facts)
    }
}

/// Run a query on the database. Returns `true` on success.
/// The `EGraph` is mutable because normalizing the query creates new names.
pub fn query<F>(egraph: &mut EGraph, facts: &Facts, mut callback: F) -> Result<(), TypeError>
where
    F: FnMut(&HashMap<&'static str, Value>) -> Result<(), ()>,
{
    let facts = egraph
        .type_info
        .typecheck_facts(&mut egraph.symbol_gen, &facts.0)?;
    let facts = crate::remove_globals::remove_globals_facts(&facts);

    let rule = ast::ResolvedRule {
        span: DUMMY_SPAN.clone(),
        head: ResolvedActions::default(),
        body: facts,
    };
    let core_rule = rule.to_canonicalized_core_rule(&egraph.type_info, &mut egraph.symbol_gen)?;
    let query = core_rule.body;

    let ordering = &query.get_vars();
    let query = egraph.compile_gj_query(query, ordering);

    let mut map = HashMap::default();
    let f = |values: &[Value]| -> Result<(), ()> {
        map.clear();
        for (i, x) in values.iter().enumerate() {
            map.insert(ordering[i].name.into(), *x);
        }
        callback(&map)
    };

    egraph.run_query(&query, 0, false, f);

    Ok(())
}

pub fn serialize(egraph: &EGraph, config: SerializeConfig) {
    egraph.serialize(config);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_test_database() -> Result<EGraph, Error> {
        let mut egraph = EGraph::default();
        egraph.parse_and_run_program(
            None,
            "
(function fib (i64) i64)
(set (fib 0) 0)
(set (fib 1) 1)
(rule (
	(= f0 (fib x))
    (= f1 (fib (+ x 1)))
) (
	(set (fib (+ x 2)) (+ f0 f1))
))
(run 10)
        ",
        )?;
        Ok(egraph)
    }

    #[test]
    fn test_query() -> Result<(), Error> {
        use expr::*;
        use fact::*;

        let mut egraph = build_test_database()?;

        let facts = facts(vec![
            equals(call("fib", vec![var("x")]), var("y")),
            equals(var("y"), int(13)),
        ]);

        let mut results = Vec::new();
        let callback = |values: &HashMap<&'static str, Value>| {
            results.push(values.clone());
            Ok(())
        };

        query(&mut egraph, &facts, callback).unwrap();

        let mut expected = HashMap::default();
        expected.insert("x", 7.into());
        assert_eq!(results, vec![expected]);

        Ok(())
    }
}
