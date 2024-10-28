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

pub fn run_rule(
    egraph: &mut EGraph,
    vars: &[(&str, ArcSort)],
    facts: Facts<Symbol, Symbol>,
    func: impl Fn(&[Value], (&[ArcSort], &ArcSort), &mut EGraph) + 'static,
) -> Result<(), Error> {
    let ruleset = rule(egraph, vars, facts, func)?;
    egraph.run_program(vec![Command::RunSchedule(Schedule::Run(
        DUMMY_SPAN.clone(),
        RunConfig {
            ruleset,
            until: None,
        },
    ))])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

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

        let results = Rc::new(RefCell::new(Vec::new()));
        let results_clone = results.clone();

        run_rule(
            &mut egraph,
            &[("x", sort::int()), ("y", sort::int())],
            facts(vec![
                equals(call("fib", vec![var("x")]), var("y")),
                equals(var("y"), int(13)),
            ]),
            move |values: &[Value], _, _| {
                let [x, y] = values else { unreachable!() };
                results_clone.borrow_mut().push((*x, *y));
            },
        )?;

        assert_eq!(*results.borrow(), [(Value::from(7), Value::from(13))]);

        Ok(())
    }
}
