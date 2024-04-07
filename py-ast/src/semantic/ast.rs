use super::mangle::Mangler;
use super::*;
use crate::ir;
use crate::parse;
use crate::semantic;
use terl::*;

pub trait Ast<S: Scope = ModScope>: ParseUnit {
    type Forward;

    /// divided [`PU`] into [`ParseUnit::Target`] and [`Span`] becase
    /// variants from [`crate::complex_pu`] isnot [`PU`], and the [`Span`]
    /// was stored in the enum
    fn to_ast(s: &Self::Target, span: Span, scope: &mut S) -> Result<Self::Forward>;
}

impl<M: Mangler> Ast<ModScope<M>> for parse::Item {
    type Forward = ();

    fn to_ast(stmt: &Self::Target, span: Span, scope: &mut ModScope<M>) -> Result<Self::Forward> {
        match stmt {
            parse::Item::FnDefine(fn_define) => {
                scope.to_ast_inner::<parse::FnDefine>(fn_define, span)?;
            }
            parse::Item::Comment(_) => {}
        }
        Ok(())
    }
}

impl<M: Mangler> Ast<ModScope<M>> for parse::FnDefine {
    type Forward = ();

    fn to_ast(
        fn_define: &Self::Target,
        span: Span,
        scope: &mut ModScope<M>,
    ) -> Result<Self::Forward> {
        let name = fn_define.name.to_string();

        let ty: ir::TypeDefine = fn_define.ty.to_ast_ty()?;

        let params = fn_define
            .params
            .params
            .iter()
            .try_fold(Vec::new(), |mut vec, pu| {
                let ty = pu.ty.to_ast_ty()?;
                let name = pu.name.clone();
                vec.push(ir::Parameter {
                    ty,
                    name: name.to_string(),
                });
                Result::Ok(vec)
            })?;

        let sign_params =
            params
                .iter()
                .cloned()
                .enumerate()
                .try_fold(Vec::new(), |mut vec, (idx, param)| {
                    let raw = &fn_define.params.params[idx];
                    let param = semantic::Param {
                        name: param.name,
                        ty: param.ty,
                        loc: raw.get_span(),
                    };
                    vec.push(param);
                    Result::Ok(vec)
                })?;

        let fn_sign = semantic::FnSign {
            ty: ty.clone(),
            params: sign_params,
            loc: span,
        };

        // generate ast
        scope.create_fn(name, fn_sign, |scope| {
            // scope.regist_params(params_iter);
            for stmt in &fn_define.codes.stmts {
                scope.to_ast(stmt)?;
            }
            Ok(())
        })?;
        // scope.push_stmt(ir::Statement::FnDefine(ir::FnDefine {
        //     ty,
        //     name,
        //     params,
        //     body,
        // }));

        Ok(())
    }
}

impl<M: Mangler> Ast<ModScope<M>> for parse::Statement {
    type Forward = ();

    fn to_ast(stmt: &Self::Target, span: Span, scope: &mut ModScope<M>) -> Result<Self::Forward> {
        match stmt {
            parse::Statement::FnCallStmt(fn_call) => {
                scope.to_ast(fn_call)?;
            }
            parse::Statement::VarStoreStmt(var_store) => {
                scope.to_ast(var_store)?;
            }
            parse::Statement::VarDefineStmt(var_define) => {
                scope.to_ast(var_define)?;
            }
            parse::Statement::If(if_) => {
                scope.to_ast_inner::<parse::If>(if_, span)?;
            }
            parse::Statement::While(while_) => {
                scope.to_ast_inner::<parse::While>(while_, span)?;
            }
            parse::Statement::Return(return_) => {
                scope.to_ast_inner::<parse::Return>(return_, span)?;
            }
            parse::Statement::CodeBlock(block) => {
                scope.to_ast_inner::<parse::CodeBlock>(block, span)?;
            }
            parse::Statement::Comment(_) => {}
        }
        Ok(())
    }
}

impl<M: Mangler> Ast<ModScope<M>> for parse::FnCall {
    type Forward = mir::Variable;

    fn to_ast(
        fn_call: &Self::Target,
        span: Span,
        scope: &mut ModScope<M>,
    ) -> Result<Self::Forward> {
        let (tys, vals): (Vec<_>, Vec<_>) = fn_call
            .args
            .args
            .iter()
            .try_fold(vec![], |mut args, expr| {
                let expr = scope.to_ast(expr)?;
                args.push((expr.ty, expr.val));
                Result::Ok(args)
            })?
            .into_iter()
            .unzip();

        let overloads = scope.function_overload_declare(&fn_call.fn_name);

        let Some(fn_def) = scope.search_fn(&fn_call.fn_name) else {
            return span.throw("use of undefined function");
        };

        // TOOD: overload support
        let fn_def = &fn_def.overloads[0];
        let params = &fn_def.params;

        if params.len() != vals.len() {
            return span.throw(format!(
                "function {} exprct {} arguments, but {} arguments passed in",
                *fn_call.fn_name,
                params.len(),
                vals.len()
            ));
        }

        for arg_idx in 0..vals.len() {
            if tys[arg_idx] != params[arg_idx].ty {
                return fn_call.args.args[arg_idx].throw(format!(
                    "expected type {}, but found type {}",
                    params[arg_idx].ty, tys[arg_idx]
                ));
            }
        }

        let fn_call = ir::FnCall {
            fn_name: fn_call.fn_name.to_string(),
            args: vals.into_iter().collect(),
        };

        let ty = fn_def.ty.clone();
        let fn_call = ir::AtomicExpr::FnCall(fn_call);
        Ok(TypedExpr::new(ty, fn_call))
    }
}

impl<M: Mangler> Ast<ModScope<M>> for parse::VarStore {
    type Forward = ();

    fn to_ast(
        var_store: &Self::Target,
        span: Span,
        scope: &mut ModScope<M>,
    ) -> Result<Self::Forward> {
        let name = var_store.name.clone();
        // function parameters are inmutable

        let val = scope.to_ast(&var_store.assign.val)?;
        let Some((var_def, mutable)) = scope.search_var(&name) else {
            return span.throw(format!("use of undefined variable {}", *name));
        };
        if !mutable {
            return span.throw(format!("cant assign to a immmutable variable {}", *name));
        }
        if var_def.ty != val.ty {
            return span.throw(format!(
                "tring to assign to variable with type {} from type {}",
                var_def.ty, val.ty
            ));
        }
        scope.push_stmt(ir::VarStore {
            name: name.to_string(),
            val: val.val,
        });
        Ok(())
    }
}

impl<M: Mangler> Ast<ModScope<M>> for parse::VarDefine {
    type Forward = ();

    fn to_ast(
        var_define: &Self::Target,
        span: Span,
        scope: &mut ModScope<M>,
    ) -> Result<Self::Forward> {
        // TODO: testfor if  ty exist
        let ty = var_define.ty.to_ast_ty()?;
        let name = var_define.name.to_string();
        let init = match &var_define.init {
            Some(init) => {
                let init = scope.to_ast(&init.val)?;
                if init.ty != ty {
                    return span.throw(format!(
                        "tring to define a variable with type {} from type {}",
                        ty, init.ty
                    ));
                }
                Some(init.val)
            }
            None => None,
        };

        scope.regist_var(name.clone(), semantic::VarDef::new(ty.clone(), span));

        scope.push_stmt(ir::VarDefine { ty, name, init });

        Ok(())
    }
}

impl<M: Mangler> Ast<ModScope<M>> for parse::If {
    type Forward = ();

    fn to_ast(if_: &Self::Target, _span: Span, scope: &mut ModScope<M>) -> Result<Self::Forward> {
        let mut branches = vec![scope.to_ast(&if_.ruo4)?];
        for chain in &if_.chains {
            match &**chain {
                parse::ChainIf::AtomicElseIf(atomic) => {
                    branches.push(scope.to_ast(&atomic.ruo4)?);
                }
                parse::ChainIf::AtomicElse(else_) => {
                    let else_ = scope.to_ast(&else_.block)?;
                    scope.push_stmt(ir::Statement::If(ir::If {
                        branches,
                        else_: Some(else_),
                    }));
                    return Ok(());
                }
            }
        }
        scope.push_stmt(ir::Statement::If(ir::If {
            branches,
            else_: None,
        }));
        Ok(())
    }
}

impl<M: Mangler> Ast<ModScope<M>> for parse::While {
    type Forward = ();

    fn to_ast(
        while_: &Self::Target,
        _span: Span,
        scope: &mut ModScope<M>,
    ) -> Result<Self::Forward> {
        let cond = scope.to_ast(&while_.conds)?;
        let body = scope.to_ast(&while_.block)?;
        scope.push_stmt(ir::Statement::While(ir::While { cond, body }));
        Ok(())
    }
}

impl<M: Mangler> Ast<ModScope<M>> for parse::AtomicIf {
    type Forward = ir::IfBranch;

    fn to_ast(
        atomic: &Self::Target,
        _span: Span,
        scope: &mut ModScope<M>,
    ) -> Result<Self::Forward> {
        let cond = scope.to_ast(&atomic.conds)?;
        let body = scope.to_ast(&atomic.block)?;
        Result::Ok(ir::IfBranch { cond, body })
    }
}

impl<M: Mangler> Ast<ModScope<M>> for parse::Return {
    type Forward = ();

    fn to_ast(
        return_: &Self::Target,
        _span: Span,
        scope: &mut ModScope<M>,
    ) -> Result<Self::Forward> {
        let val = match &return_.val {
            Some(val) => Some(scope.to_ast(val)?),
            None => None,
        };
        scope.push_stmt(ir::Statement::Return(ir::Return {
            val: val.map(|v| v.val),
        }));
        Ok(())
    }
}

impl<M: Mangler> Ast<ModScope<M>> for parse::CodeBlock {
    type Forward = ir::Statements;

    fn to_ast(block: &Self::Target, _span: Span, scope: &mut ModScope<M>) -> Result<Self::Forward> {
        scope
            .spoce(|scope| {
                for stmt in &block.stmts {
                    scope.to_ast(stmt)?;
                }
                Ok(())
            })
            .map(|(v, _)| v)
    }
}

// TODO: condition`s`
impl<M: Mangler> Ast<ModScope<M>> for parse::Arguments {
    type Forward = ir::Condition;

    fn to_ast(cond: &Self::Target, _span: Span, scope: &mut ModScope<M>) -> Result<Self::Forward> {
        let (compute, last_cond) = scope.spoce(|scope| {
            let mut last_cond = scope.to_ast(&cond.args[0])?;
            for arg in cond.args.iter().skip(1) {
                last_cond = scope.to_ast(arg)?;
            }
            Result::Ok(last_cond)
        })?;

        if last_cond.ty != ir::PrimitiveType::Bool {
            return cond.args.last().unwrap().throw("condition must be boolean");
        }

        Result::Ok(ir::Condition {
            val: last_cond.val,
            compute,
        })
    }
}

impl<M: Mangler> Ast<ModScope<M>> for parse::Expr {
    type Forward = mir::Variable;

    fn to_ast(expr: &Self::Target, span: Span, scope: &mut ModScope<M>) -> Result<Self::Forward> {
        match expr {
            parse::Expr::Atomic(atomic) => parse::AtomicExpr::to_ast(atomic, span, scope),
            parse::Expr::Binary(l, o, r) => {
                let l = scope.to_ast(l)?;
                let r = scope.to_ast(r)?;

                // TODO: type declaration
                if l.ty != r.ty {
                    return span.throw(format!(
                        "operator around different type: `{}` and `{}`!",
                        l.ty, r.ty
                    ));
                }

                // TODO: operator -> function call
                if !l.ty.is_primitive() {
                    return span.throw("only operator around primitive types are supported");
                }

                let ty = if o.ty() == crate::ops::OperatorTypes::CompareOperator {
                    ir::PrimitiveType::Bool
                } else {
                    l.ty.clone().try_into().unwrap()
                };

                let expr = scope.push_compute(ty, ir::OperateExpr::binary(o.take(), l.val, r.val));
                Result::Ok(TypedExpr::new(ty, expr))
            }
        }
    }
}

impl<M: Mangler> Ast<ModScope<M>> for parse::AtomicExpr {
    type Forward = mir::Variable;

    fn to_ast(atomic: &Self::Target, span: Span, scope: &mut ModScope<M>) -> Result<Self::Forward> {
        let atomic = match atomic {
            // atomics
            parse::AtomicExpr::CharLiteral(char) => mir::AtomicExpr::Char(char.parsed),
            parse::AtomicExpr::StringLiteral(str) => mir::AtomicExpr::String(str.parsed.clone()),
            parse::AtomicExpr::NumberLiteral(n) => match n {
                parse::NumberLiteral::Float { number, .. } => mir::AtomicExpr::Float(*number),
                parse::NumberLiteral::Digit { number, .. } => mir::AtomicExpr::Integer(*number),
            },

            parse::AtomicExpr::FnCall(fn_call) => {
                return parse::FnCall::to_ast(fn_call, span, scope)
            }
            parse::AtomicExpr::Variable(var) => {
                let Some(def) = scope.search_var(var) else {
                    return span.throw("use of undefined variable");
                };

                todo!()
            }

            // here, this is incorrect because operators may be overloadn
            // all operator overloadn must be casted into function calling here but primitives
            parse::AtomicExpr::UnaryExpr(unary) => {
                // let l = scope.to_ast(&unary.expr)?;
                // if !l.ty.is_primitive() {
                //     return span.throw("only operator around primitive types are supported");
                // }
                // let ty = ir::PrimitiveType::try_from(l.ty).unwrap();
                // let expr = scope.push_compute(ty, ir::OperateExpr::unary(*unary.operator, l.val));
                // Result::Ok(TypedExpr::new(ty, expr))
                todo!()
            }
            parse::AtomicExpr::BracketExpr(expr) => return scope.to_ast(&expr.expr),
            parse::AtomicExpr::Initialization(_) => todo!("how to do???"),
        };

        let benches = mir::Variable::atomic_benches::<M>(&atomic);
        let ty = scope.delcare(benches)?;
        Ok(mir::Variable::new_normal(atomic, ty))
    }
}
