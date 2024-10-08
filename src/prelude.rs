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

/// Run a query on the database.
pub fn query(egraph: &mut EGraph, facts: &Facts) -> Result<QueryResults, TypeError> {
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

    let mut results = QueryResults {
        vars: VarOrdering(ordering.iter().map(|v| v.name.into()).collect()),
        n_matches: 0,
        data: vec![],
    };
    egraph.run_query(&query, 0, false, |values| {
        results.n_matches += 1;
        results.data.extend(values);
        Ok(())
    });
    assert_eq!(results.n_matches * ordering.len(), results.data.len());
    Ok(results)
}

/// The result of running `query`.
pub struct QueryResults {
    pub vars: VarOrdering,
    pub n_matches: usize,
    data: Vec<Value>,
}

impl QueryResults {
    /// Returns an iterator over the results of each query match
    /// as a slice of `Value`s. The values are in the ordering
    /// defined by `vars`; see `VarOrdering::zip`.
    pub fn iter(&self) -> impl Iterator<Item = &[Value]> {
        self.data.chunks(self.vars.0.len())
    }
}

/// A list of variable names.
pub struct VarOrdering(pub Vec<&'static str>);

impl VarOrdering {
    /// Given a slice of values, attach variable names to the values.
    /// This is useful for e.g. collecting into a map.
    pub fn zip<'a>(
        &'a self,
        values: &'a [Value],
    ) -> impl Iterator<Item = (&'static str, Value)> + 'a {
        self.0.iter().copied().zip(values.iter().copied())
    }
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

        let results = query(&mut egraph, &facts)?;

        assert!(results.data.len() == 1);
        for values in results.iter() {
            assert!(values.len() == 1);
            for (var, val) in results.vars.zip(values) {
                assert_eq!(var, "x");
                assert_eq!(val, 7.into());
            }
        }

        Ok(())
    }
}
