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

    pub fn facts(facts: Vec<Fact>) -> Facts<Symbol, Symbol> {
        Facts(facts)
    }
}

pub mod sort {
    use super::*;

    pub fn int() -> ArcSort {
        Arc::new(I64Sort)
    }
}

struct RustRuleRhs<F: Fn(&[Value], (&[ArcSort], &ArcSort), &mut EGraph)> {
    name: Symbol,
    input: Vec<ArcSort>,
    func: F,
}

impl<F: Fn(&[Value], (&[ArcSort], &ArcSort), &mut EGraph)> PrimitiveLike for RustRuleRhs<F> {
    fn name(&self) -> Symbol {
        self.name
    }

    fn get_type_constraints(&self, span: &Span) -> Box<dyn TypeConstraint> {
        let sorts: Vec<_> = self
            .input
            .iter()
            .chain(once(&(Arc::new(UnitSort) as Arc<dyn Sort>)))
            .cloned()
            .collect();
        SimpleTypeConstraint::new(self.name(), sorts, span.clone()).into_box()
    }
    fn apply(
        &self,
        values: &[Value],
        sorts: (&[ArcSort], &ArcSort),
        egraph: Option<&mut EGraph>,
    ) -> Option<Value> {
        let egraph = egraph.expect("RustRuleRhs should not be used in a query");
        (self.func)(values, sorts, egraph);
        Some(Value::unit())
    }
}

/// Add a rule to the e-graph. Returns the ruleset name.
pub fn rule(
    egraph: &mut EGraph,
    vars: &[(&str, ArcSort)],
    facts: Facts<Symbol, Symbol>,
    func: impl Fn(&[Value], (&[ArcSort], &ArcSort), &mut EGraph) + 'static,
) -> Result<Symbol, Error> {
    let prim_name = egraph.symbol_gen.fresh(&Symbol::from("rust_rule_prim"));
    egraph.add_primitive(RustRuleRhs {
        name: prim_name,
        input: vars.iter().map(|(_, s)| s.clone()).collect(),
        func,
    });

    let rule = Rule {
        span: DUMMY_SPAN.clone(),
        head: GenericActions(vec![GenericAction::Expr(
            DUMMY_SPAN.clone(),
            expr::call(
                prim_name.into(),
                vars.iter().map(|(v, _)| expr::var(v)).collect(),
            ),
        )]),
        body: facts.0,
    };

    let rule_name = format!("{}", rule).into();
    let ruleset = egraph.symbol_gen.fresh(&"rust_rule_ruleset".into());
    egraph.run_program(vec![
        Command::AddRuleset(ruleset),
        Command::Rule {
            name: rule_name,
            rule,
            ruleset,
        },
    ])?;

    Ok(ruleset)
}

pub fn run_ruleset(egraph: &mut EGraph, ruleset: Symbol) -> Result<(), Error> {
    egraph.run_program(vec![Command::RunSchedule(Schedule::Run(
        DUMMY_SPAN.clone(),
        RunConfig {
            ruleset,
            until: None,
        },
    ))])?;
    Ok(())
}

pub fn query(
    egraph: &mut EGraph,
    vars: &[(&str, ArcSort)],
    facts: Facts<Symbol, Symbol>,
) -> Result<Vec<Vec<Value>>, Error> {
    use std::{cell::RefCell, rc::Rc};

    let results = Rc::new(RefCell::new(Vec::new()));
    let results_weak = Rc::downgrade(&results);

    let ruleset = rule(
        egraph,
        vars,
        facts,
        move |values, _, _| match results_weak.upgrade() {
            Some(rc) => rc.borrow_mut().push(values.to_vec()),
            None => panic!("one-shot rule was called twice"),
        },
    )?;
    run_ruleset(egraph, ruleset)?;

    match Rc::into_inner(results) {
        Some(refcell) => Ok(refcell.into_inner()),
        None => panic!("results_weak.upgrade() was not dropped"),
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
    fn rust_api_query() -> Result<(), Error> {
        use expr::*;
        use fact::*;

        let mut egraph = build_test_database()?;

        let results = query(
            &mut egraph,
            &[("x", sort::int()), ("y", sort::int())],
            facts(vec![
                equals(call("fib", vec![var("x")]), var("y")),
                equals(var("y"), int(13)),
            ]),
        )?;

        assert_eq!(results, [[Value::from(7), Value::from(13)]]);

        Ok(())
    }

    #[test]
    fn rust_api_rule() -> Result<(), Error> {
        use expr::*;
        use fact::*;

        let mut egraph = build_test_database()?;

        let big_number = 20;

        // check that `fib` does not contain `20`
        let results = query(
            &mut egraph,
            &[("f", sort::int())],
            facts(vec![equals(call("fib", vec![int(big_number)]), var("f"))]),
        )?;

        assert!(results.is_empty());

        // add the rule from `build_test_database` to the egraph with a handle
        let ruleset = rule(
            &mut egraph,
            &[("x", sort::int()), ("f0", sort::int()), ("f1", sort::int())],
            facts(vec![
                equals(var("f0"), call("fib", vec![var("x")])),
                equals(
                    var("f1"),
                    call("fib", vec![call("+", vec![var("x"), int(1)])]),
                ),
            ]),
            |values, _, egraph| {
                let [x, f0, f1] = values else { unreachable!() };
                let a = Value::from(i64::load(&I64Sort, x) + 2);
                let b = Value::from(i64::load(&I64Sort, f0) + i64::load(&I64Sort, f1));
                egraph.functions[&Symbol::from("fib")].insert(&[a], b, egraph.timestamp);
            },
        )?;

        // run that rule 10 times
        for _ in 0..10 {
            run_ruleset(&mut egraph, ruleset)?;
        }

        // check that `fib` now contains `20`
        let results = query(
            &mut egraph,
            &[("f", sort::int())],
            facts(vec![equals(call("fib", vec![int(big_number)]), var("f"))]),
        )?;
        assert_eq!(results, [[Value::from(6765)]]);

        Ok(())
    }
}