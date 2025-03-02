use crate::context::{AnalyzerContext, NamedThing};
use crate::db::{Analysis, AnalyzerDb};
use crate::errors;
use crate::namespace::items::{
    self, ContractFieldId, ContractId, DepGraph, DepGraphWrapper, DepLocality, EventId, FunctionId,
    Item, TypeDef,
};
use crate::namespace::scopes::ItemScope;
use crate::namespace::types::{self, Contract, Struct, Type};
use crate::traversal::types::type_desc;
use fe_common::diagnostics::Label;
use fe_parser::ast;
use indexmap::map::{Entry, IndexMap};
use smol_str::SmolStr;
use std::rc::Rc;

/// A `Vec` of every function defined in the contract, including duplicates and the init function.
pub fn contract_all_functions(db: &dyn AnalyzerDb, contract: ContractId) -> Rc<Vec<FunctionId>> {
    let module = contract.module(db);
    let body = &contract.data(db).ast.kind.body;
    Rc::new(
        body.iter()
            .filter_map(|stmt| match stmt {
                ast::ContractStmt::Event(_) => None,
                ast::ContractStmt::Function(node) => {
                    Some(db.intern_function(Rc::new(items::Function {
                        ast: node.clone(),
                        module,
                        parent: Some(items::Class::Contract(contract)),
                    })))
                }
            })
            .collect(),
    )
}

pub fn contract_function_map(
    db: &dyn AnalyzerDb,
    contract: ContractId,
) -> Analysis<Rc<IndexMap<SmolStr, FunctionId>>> {
    let mut scope = ItemScope::new(db, contract.module(db));
    let mut map = IndexMap::<SmolStr, FunctionId>::new();

    for func in db.contract_all_functions(contract).iter() {
        let def = &func.data(db).ast;
        let def_name = def.name();
        if def_name == "__init__" || def_name == "__call__" {
            continue;
        }

        if let Some(event) = contract.event(db, def_name) {
            scope.name_conflict_error(
                "function",
                def_name,
                &NamedThing::Item(Item::Event(event)),
                Some(event.name_span(db)),
                def.kind.name.span,
            );
            continue;
        }

        if let Some(named_item) = scope.resolve_name(def_name) {
            scope.name_conflict_error(
                "function",
                def_name,
                &named_item,
                named_item.name_span(db),
                def.kind.name.span,
            );
            continue;
        }

        match map.entry(def.name().into()) {
            Entry::Occupied(entry) => {
                scope.duplicate_name_error(
                    &format!(
                        "duplicate function names in `contract {}`",
                        contract.name(db),
                    ),
                    entry.key(),
                    entry.get().data(db).ast.span,
                    def.span,
                );
            }
            Entry::Vacant(entry) => {
                entry.insert(*func);
            }
        }
    }
    Analysis {
        value: Rc::new(map),
        diagnostics: Rc::new(scope.diagnostics),
    }
}

pub fn contract_public_function_map(
    db: &dyn AnalyzerDb,
    contract: ContractId,
) -> Rc<IndexMap<SmolStr, FunctionId>> {
    Rc::new(
        contract
            .functions(db)
            .iter()
            .filter_map(|(name, func)| func.is_public(db).then(|| (name.clone(), *func)))
            .collect(),
    )
}

pub fn contract_init_function(
    db: &dyn AnalyzerDb,
    contract: ContractId,
) -> Analysis<Option<FunctionId>> {
    let all_fns = db.contract_all_functions(contract);
    let mut init_fns = all_fns.iter().filter_map(|func| {
        let def = &func.data(db).ast;
        (def.name() == "__init__").then(|| (func, def.span))
    });

    let mut diagnostics = vec![];

    let first_def = init_fns.next();
    if let Some((_, dupe_span)) = init_fns.next() {
        let mut labels = vec![
            Label::primary(first_def.unwrap().1, "`__init__` first defined here"),
            Label::secondary(dupe_span, "`init` redefined here"),
        ];
        for (_, dupe_span) in init_fns {
            labels.push(Label::secondary(dupe_span, "`__init__` redefined here"));
        }
        diagnostics.push(errors::fancy_error(
            &format!(
                "`fn __init__()` is defined multiple times in `contract {}`",
                contract.name(db),
            ),
            labels,
            vec![],
        ));
    }

    if let Some((id, span)) = first_def {
        // `__init__` must be `pub`.
        // Return type is checked in `queries::functions::function_signature`.
        if !id.is_public(db) {
            diagnostics.push(errors::fancy_error(
                "`__init__` function is not public",
                vec![Label::primary(span, "`__init__` function must be public")],
                vec![
                    "Hint: Add the `pub` modifier.".to_string(),
                    "Example: `pub fn __init__():`".to_string(),
                ],
            ));
        }
    }

    Analysis {
        value: first_def.map(|(id, _span)| *id),
        diagnostics: Rc::new(diagnostics),
    }
}

pub fn contract_call_function(
    db: &dyn AnalyzerDb,
    contract: ContractId,
) -> Analysis<Option<FunctionId>> {
    let all_fns = db.contract_all_functions(contract);
    let mut call_fns = all_fns.iter().filter_map(|func| {
        let def = &func.data(db).ast;
        (def.name() == "__call__").then(|| (func, def.span))
    });

    let mut diagnostics = vec![];

    let first_def = call_fns.next();
    if let Some((_, dupe_span)) = call_fns.next() {
        let mut labels = vec![
            Label::primary(first_def.unwrap().1, "`__call__` first defined here"),
            Label::secondary(dupe_span, "`__call__` redefined here"),
        ];
        for (_, dupe_span) in call_fns {
            labels.push(Label::secondary(dupe_span, "`__call__` redefined here"));
        }
        diagnostics.push(errors::fancy_error(
            &format!(
                "`fn __call__()` is defined multiple times in `contract {}`",
                contract.name(db),
            ),
            labels,
            vec![],
        ));
    }

    if let Some((id, span)) = first_def {
        // `__call__` must be `pub`.
        // Return type is checked in `queries::functions::function_signature`.
        if !id.is_public(db) {
            diagnostics.push(errors::fancy_error(
                "`__call__` function is not public",
                vec![Label::primary(span, "`__call__` function must be public")],
                vec![
                    "Hint: Add the `pub` modifier.".to_string(),
                    "Example: `pub fn __call__():`".to_string(),
                ],
            ));
        }
    }

    if let Some((_id, init_span)) = first_def {
        for func in all_fns.iter() {
            let name = func.name(db);
            if func.is_public(db) && name != "__init__" && name != "__call__" {
                diagnostics.push(errors::fancy_error(
                    "`pub` not allowed if `__call__` is defined",
                    vec![
                        Label::primary(func.name_span(db), &format!("`{}` can't be public", name)),
                        Label::secondary(init_span, "`__call__` defined here"),
                    ],
                    vec![
                        "The `__call__` function replaces the default function dispatcher, which makes `pub` modifiers obsolete.".to_string(),
                        "Hint: Remove the `pub` modifier or `__call__` function.".to_string(),
                    ],
                ));
            }
        }
    }

    Analysis {
        value: first_def.map(|(id, _span)| *id),
        diagnostics: Rc::new(diagnostics),
    }
}

/// A `Vec` of all events defined within the contract, including those with duplicate names.
pub fn contract_all_events(db: &dyn AnalyzerDb, contract: ContractId) -> Rc<Vec<EventId>> {
    let body = &contract.data(db).ast.kind.body;
    Rc::new(
        body.iter()
            .filter_map(|stmt| match stmt {
                ast::ContractStmt::Function(_) => None,
                ast::ContractStmt::Event(node) => Some(db.intern_event(Rc::new(items::Event {
                    ast: node.clone(),
                    contract,
                }))),
            })
            .collect(),
    )
}

pub fn contract_event_map(
    db: &dyn AnalyzerDb,
    contract: ContractId,
) -> Analysis<Rc<IndexMap<SmolStr, EventId>>> {
    let mut scope = ItemScope::new(db, contract.module(db));
    let mut map = IndexMap::<SmolStr, EventId>::new();

    let contract_name = contract.name(db);
    for event in db.contract_all_events(contract).iter() {
        let node = &event.data(db).ast;

        match map.entry(node.name().into()) {
            Entry::Occupied(entry) => {
                scope.duplicate_name_error(
                    &format!("duplicate event names in `contract {}`", contract_name,),
                    entry.key(),
                    entry.get().data(db).ast.span,
                    node.span,
                );
            }
            Entry::Vacant(entry) => {
                entry.insert(*event);
            }
        }
    }

    Analysis {
        value: Rc::new(map),
        diagnostics: Rc::new(scope.diagnostics),
    }
}

/// All field ids, including those with duplicate names
pub fn contract_all_fields(db: &dyn AnalyzerDb, contract: ContractId) -> Rc<Vec<ContractFieldId>> {
    let fields = contract
        .data(db)
        .ast
        .kind
        .fields
        .iter()
        .map(|node| {
            db.intern_contract_field(Rc::new(items::ContractField {
                ast: node.clone(),
                parent: contract,
            }))
        })
        .collect();
    Rc::new(fields)
}

pub fn contract_field_map(
    db: &dyn AnalyzerDb,
    contract: ContractId,
) -> Analysis<Rc<IndexMap<SmolStr, ContractFieldId>>> {
    let mut scope = ItemScope::new(db, contract.module(db));
    let mut map = IndexMap::<SmolStr, ContractFieldId>::new();

    let contract_name = contract.name(db);
    for field in db.contract_all_fields(contract).iter() {
        let node = &field.data(db).ast;

        match map.entry(node.name().into()) {
            Entry::Occupied(entry) => {
                scope.duplicate_name_error(
                    &format!("duplicate field names in `contract {}`", contract_name,),
                    entry.key(),
                    entry.get().data(db).ast.span,
                    node.span,
                );
            }
            Entry::Vacant(entry) => {
                entry.insert(*field);
            }
        }
    }

    Analysis {
        value: Rc::new(map),
        diagnostics: Rc::new(scope.diagnostics),
    }
}

pub fn contract_field_type(
    db: &dyn AnalyzerDb,
    field: ContractFieldId,
) -> Analysis<Result<types::Type, errors::TypeError>> {
    let mut scope = ItemScope::new(db, field.data(db).parent.module(db));
    let typ = type_desc(&mut scope, &field.data(db).ast.kind.typ);

    let node = &field.data(db).ast;

    if node.kind.is_pub {
        scope.not_yet_implemented("contract `pub` fields", node.span);
    }
    if node.kind.is_const {
        scope.not_yet_implemented("contract `const` fields", node.span);
    }
    if let Some(value_node) = &node.kind.value {
        scope.not_yet_implemented("contract field initial value assignment", value_node.span);
    }

    Analysis {
        value: typ,
        diagnostics: Rc::new(scope.diagnostics),
    }
}

pub fn contract_dependency_graph(db: &dyn AnalyzerDb, contract: ContractId) -> DepGraphWrapper {
    // A contract depends on the types of its fields, and the things those types depend on.
    // Note that this *does not* include the contract's public function graph.
    // (See `contract_runtime_dependency_graph` below)

    let fields = contract.fields(db);
    let field_types = fields
        .values()
        .filter_map(|field| match field.typ(db).ok()? {
            // We don't want
            Type::Contract(Contract { id, .. }) => Some(Item::Type(TypeDef::Contract(id))),
            Type::Struct(Struct { id, .. }) => Some(Item::Type(TypeDef::Struct(id))),
            // TODO: when tuples can contain non-primitive items,
            // we'll have to depend on tuple element types
            _ => None,
        })
        .collect::<Vec<_>>();

    let root = Item::Type(TypeDef::Contract(contract));
    let mut graph = DepGraph::from_edges(
        field_types
            .iter()
            .map(|item| (root, *item, DepLocality::Local)),
    );

    for item in field_types {
        if let Some(subgraph) = item.dependency_graph(db) {
            graph.extend(subgraph.all_edges())
        }
    }
    DepGraphWrapper(Rc::new(graph))
}

pub fn contract_dependency_graph_cycle(
    _db: &dyn AnalyzerDb,
    _cycle: &[String],
    _contract: &ContractId,
) -> DepGraphWrapper {
    DepGraphWrapper(Rc::new(DepGraph::new()))
}

pub fn contract_runtime_dependency_graph(
    db: &dyn AnalyzerDb,
    contract: ContractId,
) -> DepGraphWrapper {
    // This is the dependency graph of the (as yet imaginary) `__call__` function, which
    // dispatches to the contract's public functions. This should be used when compiling
    // the runtime object for a contract.

    let root = Item::Type(TypeDef::Contract(contract));
    let pub_fns = contract
        .public_functions(db)
        .values()
        .map(|fun| (root, Item::Function(*fun), DepLocality::Local))
        .collect::<Vec<_>>();

    let mut graph = DepGraph::from_edges(pub_fns.iter());

    for (_, item, _) in pub_fns {
        if let Some(subgraph) = item.dependency_graph(db) {
            graph.extend(subgraph.all_edges())
        }
    }
    DepGraphWrapper(Rc::new(graph))
}

pub fn contract_runtime_dependency_graph_cycle(
    _db: &dyn AnalyzerDb,
    _cycle: &[String],
    _contract: &ContractId,
) -> DepGraphWrapper {
    DepGraphWrapper(Rc::new(DepGraph::new()))
}
