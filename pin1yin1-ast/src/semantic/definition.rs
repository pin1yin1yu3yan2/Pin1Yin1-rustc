use crate::ast;
use std::{collections::HashMap, marker::PhantomData};

#[cfg(feature = "parser")]
use crate::parse;

#[derive(Default, Debug, Clone)]
pub struct FnDefinitions<'ast> {
    pub map: HashMap<String, FnDefinition<'ast>>,
}

#[derive(Default, Debug, Clone)]
pub struct FnDefinition<'ast> {
    /// functions have same names but different signatures
    ///
    /// unsupport now
    pub overdrives: Vec<FnSign<'ast>>,
    #[cfg(feature = "parser")]
    pub raw_defines: Vec<&'ast parse::FnDefine>,
    _p: PhantomData<&'ast ()>,
}

impl<'ast> FnDefinition<'ast> {
    pub fn new(
        overdrives: Vec<FnSign<'ast>>,
        #[cfg(feature = "parser")] raw_defines: Vec<&'ast parse::FnDefine>,
    ) -> Self {
        Self {
            overdrives,
            #[cfg(feature = "parser")]
            raw_defines,
            _p: PhantomData,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FnSign<'ast> {
    pub mangle: String,
    pub ty: ast::TypeDefine,
    pub params: Vec<Parameter<'ast>>,
}

#[derive(Debug, Clone)]
pub struct Parameter<'ast> {
    pub name: String,
    #[cfg(feature = "parser")]
    pub var_def: VarDefinition<'ast>,
}

impl<'ast> std::ops::Deref for Parameter<'ast> {
    type Target = VarDefinition<'ast>;

    fn deref(&self) -> &Self::Target {
        &self.var_def
    }
}

#[derive(Default, Debug, Clone)]
pub struct VarDefinitions<'ast> {
    pub map: HashMap<String, VarDefinition<'ast>>,
}

#[derive(Debug, Clone)]
pub struct VarDefinition<'ast> {
    pub ty: ast::TypeDefine,
    #[cfg(feature = "parser")]
    pub raw_define: &'ast parse::VarDefine,
    _p: PhantomData<&'ast ()>,
}

impl<'ast> VarDefinition<'ast> {
    pub fn new(
        ty: ast::TypeDefine,
        #[cfg(feature = "parser")] raw_define: &'ast parse::VarDefine,
    ) -> Self {
        Self {
            ty,
            #[cfg(feature = "parser")]
            raw_define,
            _p: PhantomData,
        }
    }
}

#[derive(Default, Debug, Clone)]
pub struct TypeDefinitions {
    pub map: HashMap<String, TypeDefinition>,
}

#[derive(Debug, Clone)]
pub struct TypeDefinition {}
