use crate::error::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimitiveType {
    Inteiro,
    Real,
    Caractere,
    Logico,
}

impl PrimitiveType {
    pub fn label(self) -> &'static str {
        match self {
            PrimitiveType::Inteiro => "inteiro",
            PrimitiveType::Real => "real",
            PrimitiveType::Caractere => "caractere",
            PrimitiveType::Logico => "logico",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArrayRangeExpr {
    pub start: Expr,
    pub end: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldDecl {
    pub name: String,
    pub var_type: VarType,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum VarType {
    Primitive(PrimitiveType),
    Named(String, Span),
    Record {
        fields: Vec<FieldDecl>,
        span: Span,
    },
    Array {
        elem_type: Box<VarType>,
        dims: Vec<ArrayRangeExpr>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConstDecl {
    pub name: String,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeDecl {
    pub name: String,
    pub def: VarType,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VarDecl {
    pub name: String,
    pub var_type: VarType,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParamDecl {
    pub name: String,
    pub var_type: VarType,
    pub by_ref: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RoutineDecl {
    pub name: String,
    pub params: Vec<ParamDecl>,
    pub return_type: Option<VarType>,
    pub locals: Vec<VarDecl>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub title: String,
    pub consts: Vec<ConstDecl>,
    pub types: Vec<TypeDecl>,
    pub arquivo: Option<(String, Span)>,
    pub globals: Vec<VarDecl>,
    pub routines: Vec<RoutineDecl>,
    pub body: Vec<Stmt>,
    pub end_line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Plus,
    Minus,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    IntDiv,
    Mod,
    Pow,
    Eq,
    Neq,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    Xor,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Int(i64, Span),
    Real(f64, Span),
    Str(String, Span),
    Bool(bool, Span),
    Var(String, Span),
    Field {
        target: Box<Expr>,
        field: String,
        span: Span,
    },
    Index {
        target: Box<Expr>,
        indices: Vec<Expr>,
        span: Span,
    },
    Call {
        name: String,
        args: Vec<Expr>,
        span: Span,
    },
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
        span: Span,
    },
    Binary {
        left: Box<Expr>,
        op: BinaryOp,
        right: Box<Expr>,
        span: Span,
    },
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Int(_, s)
            | Expr::Real(_, s)
            | Expr::Str(_, s)
            | Expr::Bool(_, s)
            | Expr::Var(_, s)
            | Expr::Field { span: s, .. }
            | Expr::Index { span: s, .. }
            | Expr::Call { span: s, .. }
            | Expr::Unary { span: s, .. }
            | Expr::Binary { span: s, .. } => *s,
        }
    }
}

fn is_leaf_expr(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Int(_, _)
            | Expr::Real(_, _)
            | Expr::Str(_, _)
            | Expr::Bool(_, _)
            | Expr::Var(_, _)
    )
}

fn take_expr_children(node: &mut Expr, stack: &mut Vec<Expr>) {
    let dummy = || Expr::Bool(false, Span::new(0, 0, 0));
    match node {
        Expr::Int(_, _)
        | Expr::Real(_, _)
        | Expr::Str(_, _)
        | Expr::Bool(_, _)
        | Expr::Var(_, _) => {}
        Expr::Field { target, .. } => {
            stack.push(std::mem::replace(target.as_mut(), dummy()));
        }
        Expr::Index {
            target, indices, ..
        } => {
            stack.push(std::mem::replace(target.as_mut(), dummy()));
            stack.extend(std::mem::take(indices));
        }
        Expr::Call { args, .. } => {
            stack.extend(std::mem::take(args));
        }
        Expr::Unary { expr, .. } => {
            stack.push(std::mem::replace(expr.as_mut(), dummy()));
        }
        Expr::Binary { left, right, .. } => {
            stack.push(std::mem::replace(left.as_mut(), dummy()));
            stack.push(std::mem::replace(right.as_mut(), dummy()));
        }
    }
}

impl Drop for Expr {
    fn drop(&mut self) {
        let has_nested_children = match self {
            Expr::Int(_, _)
            | Expr::Real(_, _)
            | Expr::Str(_, _)
            | Expr::Bool(_, _)
            | Expr::Var(_, _) => false,
            Expr::Unary { expr, .. } => !is_leaf_expr(expr),
            Expr::Field { target, .. } => !is_leaf_expr(target),
            Expr::Binary { left, right, .. } => !is_leaf_expr(left) || !is_leaf_expr(right),
            Expr::Index {
                target, indices, ..
            } => !is_leaf_expr(target) || !indices.is_empty(),
            Expr::Call { args, .. } => !args.is_empty(),
        };
        if !has_nested_children {
            return;
        }
        let mut stack = Vec::new();
        take_expr_children(self, &mut stack);
        while let Some(mut node) = stack.pop() {
            take_expr_children(&mut node, &mut stack);
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WriteArg {
    pub expr: Expr,
    pub width: Option<Expr>,
    pub decimals: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AccessStep {
    Field { name: String, span: Span },
    Index { indices: Vec<Expr>, span: Span },
}

#[derive(Debug, Clone, PartialEq)]
pub struct LValue {
    pub base: String,
    pub base_span: Span,
    pub steps: Vec<AccessStep>,
    pub span: Span,
}

impl LValue {
    pub fn span(&self) -> Span {
        self.span
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CaseMatcher {
    Value(Expr),
    Range(Expr, Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct CaseArm {
    pub matchers: Vec<CaseMatcher>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AleatorioConfig {
    OnDefault,
    Off,
    MaxOnly(Expr),
    Range(Expr, Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub enum TimerConfig {
    On,
    Off,
    Millis(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Assign {
        target: LValue,
        value: Expr,
        span: Span,
    },
    Write {
        args: Vec<WriteArg>,
        newline: bool,
        span: Span,
    },
    Read {
        targets: Vec<LValue>,
        span: Span,
    },
    ClearScreen {
        span: Span,
    },
    If {
        cond: Expr,
        then_body: Vec<Stmt>,
        else_body: Vec<Stmt>,
        span: Span,
    },
    Switch {
        expr: Expr,
        arms: Vec<CaseArm>,
        otherwise: Vec<Stmt>,
        span: Span,
    },
    For {
        var: String,
        start: Expr,
        end: Expr,
        step: Option<Expr>,
        body: Vec<Stmt>,
        span: Span,
    },
    While {
        cond: Expr,
        body: Vec<Stmt>,
        span: Span,
    },
    Repeat {
        body: Vec<Stmt>,
        until_cond: Expr,
        span: Span,
    },
    ProcCall {
        name: String,
        args: Vec<Expr>,
        span: Span,
    },
    Return {
        value: Option<Expr>,
        span: Span,
    },
    Break {
        span: Span,
    },
    Arquivo {
        path: String,
        span: Span,
    },
    Aleatorio {
        config: AleatorioConfig,
        span: Span,
    },
    Timer {
        config: TimerConfig,
        span: Span,
    },
    Cronometro {
        enabled: bool,
        span: Span,
    },
    Eco {
        enabled: bool,
        span: Span,
    },
    Pausa {
        span: Span,
    },
    Debug {
        cond: Option<Expr>,
        span: Span,
    },
    MudaCor {
        color: Expr,
        target: Expr,
        span: Span,
    },
    Dos {
        span: Span,
    },
}

impl Stmt {
    pub fn span(&self) -> Span {
        match self {
            Stmt::Assign { span, .. }
            | Stmt::Write { span, .. }
            | Stmt::Read { span, .. }
            | Stmt::ClearScreen { span }
            | Stmt::If { span, .. }
            | Stmt::Switch { span, .. }
            | Stmt::For { span, .. }
            | Stmt::While { span, .. }
            | Stmt::Repeat { span, .. }
            | Stmt::ProcCall { span, .. }
            | Stmt::Return { span, .. }
            | Stmt::Break { span }
            | Stmt::Arquivo { span, .. }
            | Stmt::Aleatorio { span, .. }
            | Stmt::Timer { span, .. }
            | Stmt::Cronometro { span, .. }
            | Stmt::Eco { span, .. }
            | Stmt::Pausa { span }
            | Stmt::Debug { span, .. }
            | Stmt::MudaCor { span, .. }
            | Stmt::Dos { span } => *span,
        }
    }

    pub fn line(&self) -> usize {
        self.span().line
    }
}

fn take_stmt_children(node: &mut Stmt, stack: &mut Vec<Stmt>) {
    match node {
        Stmt::If {
            then_body,
            else_body,
            ..
        } => {
            stack.extend(std::mem::take(else_body));
            stack.extend(std::mem::take(then_body));
        }
        Stmt::Switch {
            arms, otherwise, ..
        } => {
            stack.extend(std::mem::take(otherwise));
            for arm in std::mem::take(arms) {
                drop(arm.matchers);
                stack.extend(arm.body);
            }
        }
        Stmt::For { body, .. } | Stmt::While { body, .. } | Stmt::Repeat { body, .. } => {
            stack.extend(std::mem::take(body));
        }
        Stmt::Assign { .. }
        | Stmt::Write { .. }
        | Stmt::Read { .. }
        | Stmt::ClearScreen { .. }
        | Stmt::ProcCall { .. }
        | Stmt::Return { .. }
        | Stmt::Break { .. }
        | Stmt::Arquivo { .. }
        | Stmt::Aleatorio { .. }
        | Stmt::Timer { .. }
        | Stmt::Cronometro { .. }
        | Stmt::Eco { .. }
        | Stmt::Pausa { .. }
        | Stmt::Debug { .. }
        | Stmt::MudaCor { .. }
        | Stmt::Dos { .. } => {}
    }
}

impl Drop for Stmt {
    fn drop(&mut self) {
        let has_children = match self {
            Stmt::If {
                then_body,
                else_body,
                ..
            } => !then_body.is_empty() || !else_body.is_empty(),
            Stmt::Switch {
                arms, otherwise, ..
            } => !arms.is_empty() || !otherwise.is_empty(),
            Stmt::For { body, .. } | Stmt::While { body, .. } | Stmt::Repeat { body, .. } => {
                !body.is_empty()
            }
            Stmt::Assign { .. }
            | Stmt::Write { .. }
            | Stmt::Read { .. }
            | Stmt::ClearScreen { .. }
            | Stmt::ProcCall { .. }
            | Stmt::Return { .. }
            | Stmt::Break { .. }
            | Stmt::Arquivo { .. }
            | Stmt::Aleatorio { .. }
            | Stmt::Timer { .. }
            | Stmt::Cronometro { .. }
            | Stmt::Eco { .. }
            | Stmt::Pausa { .. }
            | Stmt::Debug { .. }
            | Stmt::MudaCor { .. }
            | Stmt::Dos { .. } => false,
        };
        if !has_children {
            return;
        }
        let mut stack = Vec::new();
        take_stmt_children(self, &mut stack);
        while let Some(mut node) = stack.pop() {
            take_stmt_children(&mut node, &mut stack);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_deep_nested_and_left_leaning_exprs_without_stack_overflow() {
        let span = Span::new(1, 1, 1);

        // Deep unary nesting (50,000 levels)
        let mut unary = Expr::Int(1, span);
        for _ in 0..50_000 {
            unary = Expr::Unary {
                op: UnaryOp::Minus,
                expr: Box::new(unary),
                span,
            };
        }
        drop(unary);

        // Deep left-leaning binary chain (50,000 levels)
        let mut left_chain = Expr::Int(1, span);
        for _ in 0..50_000 {
            left_chain = Expr::Binary {
                left: Box::new(left_chain),
                op: BinaryOp::Add,
                right: Box::new(Expr::Int(1, span)),
                span,
            };
        }
        drop(left_chain);

        // Deep right-leaning binary chain (50,000 levels)
        let mut right_chain = Expr::Int(1, span);
        for _ in 0..50_000 {
            right_chain = Expr::Binary {
                left: Box::new(Expr::Int(1, span)),
                op: BinaryOp::Add,
                right: Box::new(right_chain),
                span,
            };
        }
        drop(right_chain);
    }

    #[test]
    fn drops_deep_nested_stmts_without_stack_overflow() {
        let span = Span::new(1, 1, 1);
        let mut stmt = Stmt::Break { span };
        for _ in 0..50_000 {
            stmt = Stmt::If {
                cond: Expr::Bool(true, span),
                then_body: vec![stmt],
                else_body: Vec::new(),
                span,
            };
        }
        drop(stmt);
    }
}
