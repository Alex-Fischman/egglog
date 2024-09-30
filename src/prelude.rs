use crate::*;

/// Run a query on the database. Returns `true` on success.
/// The `EGraph` is mutable because normalizing the query creates new names.
pub fn query(
    egraph: &mut EGraph,
    facts: &Facts,
    mut callback: impl FnMut(&HashMap<Symbol, Value>) -> Result<(), ()>,
) -> Result<(), TypeError> {
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

    let mut map: HashMap<Symbol, Value> = Default::default();
    let f = |values: &[Value]| -> Result<(), ()> {
        map.clear();
        for (i, x) in values.iter().enumerate() {
            map.insert(ordering[i].name, *x);
        }
        callback(&map)
    };

    egraph.run_query(&query, egraph.timestamp, false, f);

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
(run 7)
        ",
        )?;
        Ok(egraph)
    }

    #[test]
    fn test_query() -> Result<(), Error> {
        let mut egraph = build_test_database()?;

        let x_sym = "x".into();
        let y_sym = "y".into();
        let x = Expr::Var(DUMMY_SPAN.clone(), x_sym);
        let y = Expr::Var(DUMMY_SPAN.clone(), y_sym);
        let seven = Expr::Lit(DUMMY_SPAN.clone(), Literal::Int(7));
        let fib_x = Expr::Call(DUMMY_SPAN.clone(), "fib".into(), vec![x]);
        let facts = GenericFacts(vec![
            Fact::Eq(DUMMY_SPAN.clone(), vec![y.clone(), fib_x]),
            Fact::Eq(DUMMY_SPAN.clone(), vec![y, seven]),
        ]);

        let mut results = Vec::new();
        let callback = |values: &HashMap<Symbol, Value>| {
            results.push(values.clone());
            Ok(())
        };

        query(&mut egraph, &facts, callback).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].len(), 2);
        assert_eq!(results[0][&x_sym].bits, 13);
        assert_eq!(results[0][&y_sym].bits, 5);

        Ok(())
    }
}
