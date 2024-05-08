use std::error::Error;

use crate::scope::{AllocVariable, ComputeResult, Global, Scope, Variable};
use inkwell::{
    builder::{Builder, BuilderError},
    context::Context,
    execution_engine::ExecutionEngine,
    module::Module,
    types::{BasicType, BasicTypeEnum},
    values::{BasicValue, BasicValueEnum, FunctionValue},
};

use py_ir::ir::{self, ControlFlow};

pub struct CodeGen<'ctx> {
    context: &'ctx Context,
    module: Module<'ctx>,
    builder: Builder<'ctx>,
    current_fn: Option<FunctionValue<'ctx>>,

    pub execution_engine: ExecutionEngine<'ctx>,
    global: Global<'ctx>,
}

impl<'ctx> CodeGen<'ctx> {
    pub fn new<C: Compile>(
        context: &'ctx Context,
        module: &str,
        cus: &[C],
    ) -> Result<Self, Box<dyn Error>> {
        let module = context.create_module(module);

        let builder = context.create_builder();

        let mut cg = Self {
            execution_engine: module
                .create_jit_execution_engine(inkwell::OptimizationLevel::None)?,
            context,
            module,
            builder,
            global: Default::default(),
            current_fn: None,
        };

        for cu in cus {
            cu.generate(&mut cg)?;
        }

        Ok(cg)
    }

    pub fn regist_var<V: Variable<'ctx> + 'ctx>(&mut self, name: String, val: V) {
        self.global.regist_var(name, val)
    }

    pub fn get_var(&self, name: &str) -> &(dyn Variable<'ctx> + 'ctx) {
        self.global.get_var(name)
    }

    pub fn get_fn(&self, name: &str) -> inkwell::values::FunctionValue<'ctx> {
        self.global.get_fn(name)
    }

    pub fn regist_fn(&mut self, name: String, val: inkwell::values::FunctionValue<'ctx>) {
        self.global.regist_fn(name, val)
    }

    fn primitive_type_cast(&self, ty: &ir::PrimitiveType) -> BasicTypeEnum<'ctx> {
        match ty {
            ir::PrimitiveType::Bool => self.context.bool_type().into(),
            ir::PrimitiveType::I8 | ir::PrimitiveType::U8 => self.context.i8_type().into(),
            ir::PrimitiveType::I16 | ir::PrimitiveType::U16 => self.context.i16_type().into(),
            ir::PrimitiveType::I32 | ir::PrimitiveType::U32 => self.context.i32_type().into(),
            ir::PrimitiveType::I64 | ir::PrimitiveType::U64 => self.context.i64_type().into(),
            ir::PrimitiveType::I128 | ir::PrimitiveType::U128 => self.context.i128_type().into(),
            ir::PrimitiveType::Usize | ir::PrimitiveType::Isize => self.context.i64_type().into(),
            ir::PrimitiveType::F32 => self.context.f32_type().into(),
            ir::PrimitiveType::F64 => self.context.f64_type().into(),
        }
    }

    fn type_cast(&self, ty: &ir::TypeDefine) -> BasicTypeEnum<'ctx> {
        match ty {
            ir::TypeDefine::Primitive(ty) => self.primitive_type_cast(ty),
            ir::TypeDefine::Complex(_ty) => {
                todo!()
            }
        }
    }

    fn eval_variable(&self, var: &ir::Variable) -> Result<BasicValueEnum<'ctx>, BuilderError> {
        match var {
            ir::Variable::FnCall(fn_call) => {
                let fn_ = self.get_fn(&fn_call.fn_name);
                let args = fn_call.args.iter().try_fold(vec![], |mut vec, arg| {
                    vec.push(self.eval_variable(arg)?.into());
                    Ok(vec)
                })?;

                let val = self
                    .builder
                    .build_call(fn_, &args, &fn_call.fn_name)?
                    .try_as_basic_value()
                    // TODO: `kong1` as return type
                    .unwrap_left();
                Ok(val)
            }
            ir::Variable::Variable(variable) => self.get_var(variable).load(&self.builder),
            ir::Variable::Literal(literal, ty) => self.literal(literal, ty),
        }
    }

    fn literal(
        &self,
        literal: &ir::Literal,
        ty: &ir::PrimitiveType,
    ) -> Result<BasicValueEnum<'ctx>, BuilderError> {
        let ret = match literal {
            ir::Literal::Integer(int) if ty.is_integer() => match ty.width() {
                1 => self
                    .context
                    .bool_type()
                    .const_int(*int as _, ty.is_signed())
                    .into(),
                8 => self
                    .context
                    .i8_type()
                    .const_int(*int as _, ty.is_signed())
                    .into(),
                16 => self
                    .context
                    .i16_type()
                    .const_int(*int as _, ty.is_signed())
                    .into(),
                32 => self
                    .context
                    .i32_type()
                    .const_int(*int as _, ty.is_signed())
                    .into(),
                64 => self
                    .context
                    .i64_type()
                    .const_int(*int as _, ty.is_signed())
                    .into(),
                128 => self
                    .context
                    .i128_type()
                    .const_int(*int as _, ty.is_signed())
                    .into(),
                _ => unreachable!(),
            },
            ir::Literal::Float(float) if ty.is_float() => match ty.width() {
                32 => self.context.f32_type().const_float(*float).into(),
                64 => self.context.f64_type().const_float(*float).into(),
                _ => unreachable!(),
            },
            ir::Literal::Char(char) if ty == &ir::PrimitiveType::U32 => self
                .context
                .i32_type()
                .const_int(*char as _, ty.is_signed())
                .into(),
            _ => panic!("incorrect PrimitiveType are passed in"),
        };
        Ok(ret)
    }

    fn generate_stmts(
        &mut self,
        stmts: &[ir::Statement<ir::Variable>],
    ) -> Result<&mut Self, BuilderError> {
        for stmt in stmts {
            stmt.generate(self)?;
        }

        Ok(self)
    }

    fn scope<T, A>(&mut self, fn_: FunctionValue<'ctx>, action: A) -> T
    where
        A: FnOnce(&mut Self) -> T,
    {
        self.global.scopes.push(Scope::default());
        let mut fn_ = Some(fn_);

        std::mem::swap(&mut self.current_fn, &mut fn_);
        let t = (action)(self);
        std::mem::swap(&mut self.current_fn, &mut fn_);

        self.global.scopes.pop();
        t
    }

    pub fn llvm_ir(&self) -> String {
        self.module.print_to_string().to_string()
    }

    /// implements for [ir::Statement] will only be called in [Self::scope]
    pub fn current_fn(&self) -> FunctionValue<'ctx> {
        self.current_fn.unwrap()
    }
}

pub trait Compile {
    fn generate(&self, state: &mut CodeGen) -> Result<(), BuilderError>;
}

impl Compile for ir::Item<ir::Variable> {
    fn generate(&self, state: &mut CodeGen) -> Result<(), BuilderError> {
        match self {
            ir::Item::FnDefine(d) => d.generate(state),
        }
    }
}

impl Compile for ir::Statement<ir::Variable> {
    fn generate(&self, state: &mut CodeGen) -> Result<(), BuilderError> {
        match self {
            ir::Statement::Compute(c) => c.generate(state),
            ir::Statement::VarDefine(v) => v.generate(state),
            ir::Statement::VarStore(v) => v.generate(state),
            ir::Statement::If(i) => i.generate(state),
            ir::Statement::While(w) => w.generate(state),
            ir::Statement::Return(r) => r.generate(state),
            ir::Statement::Block(b) => state.generate_stmts(b).map(|_| ()),
        }
    }
}
impl Compile for ir::FnDefine<ir::Variable> {
    fn generate(&self, state: &mut CodeGen) -> Result<(), BuilderError> {
        let retty = state.type_cast(&self.ty);
        let param_ty = self
            .params
            .iter()
            .map(|param| state.type_cast(&param.ty).into())
            .collect::<Vec<_>>();
        let fn_ty = retty.fn_type(&param_ty, false);

        let fn_ = state.module.add_function(&self.name, fn_ty, None);
        state.regist_fn(self.name.clone(), fn_);
        state.scope(fn_, move |state| {
            let params = self
                .params
                .iter()
                .enumerate()
                .map(|(idx, param)| (param.name.clone(), fn_.get_nth_param(idx as _).unwrap()))
                .map(|(name, param)| (name, ComputeResult { val: param }));
            state.global.regist_params(params);
            let entry = state.context.append_basic_block(fn_, "entry");
            state.builder.position_at_end(entry);
            state.generate_stmts(&self.body)?;
            Ok(())
        })?;

        Ok(())
    }
}
impl Compile for ir::Compute<ir::Variable> {
    fn generate(&self, state: &mut CodeGen) -> Result<(), BuilderError> {
        let builder = &state.builder;
        match &self.eval {
            ir::OperateExpr::Unary(op, val) => {
                let val = state.eval_variable(val)?;
                let val = crate::primitive::unary_compute(builder, self.ty, *op, val, &self.name)?;
                state.regist_var(self.name.to_owned(), ComputeResult { val })
            }
            ir::OperateExpr::Binary(op, l, r) => {
                let l = state.eval_variable(l)?;
                let r = state.eval_variable(r)?;
                let val =
                    crate::primitive::binary_compute(builder, self.ty, *op, l, r, &self.name)?;
                state.regist_var(self.name.to_owned(), ComputeResult { val })
            }
        };

        Ok(())
    }
}
/// alloca, eval, store
impl Compile for ir::VarDefine<ir::Variable> {
    fn generate(&self, state: &mut CodeGen) -> Result<(), BuilderError> {
        let ty = state.type_cast(&self.ty);
        let pointer = state.builder.build_alloca(ty, &self.name)?;

        let val = AllocVariable { ty, pointer };

        if let Some(init) = &self.init {
            let init = state.eval_variable(init)?;
            val.store(&state.builder, init)?;
        }
        state.regist_var(self.name.clone(), val);
        Ok(())
    }
}
// eval, store
impl Compile for ir::VarStore<ir::Variable> {
    fn generate(&self, state: &mut CodeGen) -> Result<(), BuilderError> {
        let val = state.eval_variable(&self.val)?;
        let s = state.get_var(&self.name);
        s.store(&state.builder, val)?;
        Ok(())
    }
}
// call
impl Compile for ir::FnCall<ir::Variable> {
    fn generate(&self, state: &mut CodeGen) -> Result<(), BuilderError> {
        let fn_ = state.get_fn(&self.fn_name);
        let args = self.args.iter().try_fold(vec![], |mut vec, arg| {
            vec.push(state.eval_variable(arg)?.into());
            Ok(vec)
        })?;

        // droped
        state.builder.build_call(fn_, &args, &self.fn_name)?;
        Ok(())
    }
}
impl Compile for ir::If<ir::Variable> {
    fn generate(&self, state: &mut CodeGen) -> Result<(), BuilderError> {
        let after_exist = !self.returned();

        // note: conds.last() is `else block`(if exist), and br into codes.last()
        let conds = (0..self.branches.len() + 1)
            .map(|_| state.context.append_basic_block(state.current_fn(), ""))
            .collect::<Vec<_>>();
        let codes = (0..self.branches.len() + after_exist as usize)
            .map(|_| state.context.append_basic_block(state.current_fn(), ""))
            .collect::<Vec<_>>();

        // jump to eval first condition
        state.builder.build_unconditional_branch(conds[0])?;

        let else_block = conds.last().copied().unwrap();
        let code_after = codes.last().copied().unwrap();

        for idx in 0..self.branches.len() {
            state.builder.position_at_end(conds[idx]);
            let condition = &self.branches[idx].cond;
            state.generate_stmts(&condition.compute)?;

            let cond_val = state.eval_variable(&condition.val)?.into_int_value();
            // br <cond> <code> <else>
            state
                .builder
                .build_conditional_branch(cond_val, codes[idx], conds[idx + 1])?;

            // generate if body
            state.builder.position_at_end(codes[idx]);
            state.generate_stmts(&self.branches[idx].body)?;

            if after_exist && !self.branches[idx].returned() {
                state.builder.build_unconditional_branch(code_after)?;
            }
        }

        state.builder.position_at_end(else_block);
        if let Some(else_) = &self.else_ {
            state.generate_stmts(else_)?;
        }

        if after_exist {
            state.builder.build_unconditional_branch(code_after)?;
            state.builder.position_at_end(code_after);
        }

        Ok(())
    }
}
impl Compile for ir::While<ir::Variable> {
    fn generate(&self, state: &mut CodeGen) -> Result<(), BuilderError> {
        let cond = state.context.append_basic_block(state.current_fn(), "");
        let code = state.context.append_basic_block(state.current_fn(), "");
        let after = state.context.append_basic_block(state.current_fn(), "");

        state.builder.position_at_end(cond);
        state.generate_stmts(&self.cond.compute)?;
        let cond_val = state.eval_variable(&self.cond.val)?.into_int_value();
        state
            .builder
            .build_conditional_branch(cond_val, code, after)?;

        state.builder.position_at_end(code);
        state.generate_stmts(&self.body)?;
        state.builder.build_unconditional_branch(cond)?;

        state.builder.position_at_end(after);

        Ok(())
    }
}
impl Compile for ir::Return<ir::Variable> {
    fn generate(&self, state: &mut CodeGen) -> Result<(), BuilderError> {
        let val = match &self.val {
            Some(val) => Some(state.eval_variable(val)?),
            None => None,
        };
        state
            .builder
            .build_return(val.as_ref().map(|v| v as &dyn BasicValue))?;
        Ok(())
    }
}
