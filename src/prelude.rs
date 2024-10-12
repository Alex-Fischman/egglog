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
        num_matches: 0,
        vars: ordering.iter().map(|v| v.name.into()).collect(),
        data: vec![],
    };
    egraph.run_query(&query, 0, false, |values| {
        results.num_matches += 1;
        results.data.extend(values);
        Ok(())
    });
    assert_eq!(results.num_matches * ordering.len(), results.data.len());
    Ok(results)
}

/// The result of running `query`.
pub struct QueryResults {
    num_matches: usize,
    vars: Vec<&'static str>,
    data: Vec<Value>,
}

impl QueryResults {
    /// Iterate over the results of the query.
    pub fn iter(&self) -> impl Iterator<Item = &[Value]> {
        self.data.chunks(self.vars.len())
    }
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

        for map in results.iter() {
            assert_eq!(map, [7.into()]);
        }

        Ok(())
    }
}
