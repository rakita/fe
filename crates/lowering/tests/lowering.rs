use fe_analyzer::namespace::items::{Global, Module, ModuleContext, ModuleFileContent};
use fe_analyzer::AnalyzerDb;
use fe_common::diagnostics::print_diagnostics;
use fe_common::files::{FileStore, SourceFileId};
use fe_lowering::TestDb;
use fe_parser::ast as fe;
use insta::assert_snapshot;
use std::rc::Rc;
use wasm_bindgen_test::wasm_bindgen_test;

fn lower(src: &str, id: SourceFileId, files: &FileStore) -> fe::Module {
    let ast = parse_file(src, id, files);

    let db = TestDb::default();

    let global = Global::default();
    let global_id = db.intern_global(Rc::new(global));

    let module = Module {
        name: "test_module".into(),
        context: ModuleContext::Global(global_id),
        file_content: ModuleFileContent::File { file: id },
        ast,
    };
    let module_id = db.intern_module(Rc::new(module));

    fe_lowering::lower_module(&db, module_id).ast(&db)
}

fn parse_file(src: &str, id: SourceFileId, files: &FileStore) -> fe::Module {
    match fe_parser::parse_file(id, src) {
        Ok((module, diags)) if diags.is_empty() => module,
        Ok((_, diags)) | Err(diags) => {
            print_diagnostics(&diags, files);
            panic!("failed to parse file");
        }
    }
}

macro_rules! test_file {
    ($name:ident, $path:expr) => {
        #[test]
        #[wasm_bindgen_test]
        fn $name() {
            let mut files = FileStore::new();

            let src = test_files::fixture($path);
            let src_id = files.add_file($path, src);
            let lowered_code = format!("{}", lower(src, src_id, &files));

            if cfg!(target_arch = "wasm32") {
                fe_common::assert_snapshot_wasm!(
                    concat!("snapshots/lowering__", stringify!($name), ".snap"),
                    lowered_code
                );
            } else {
                assert_snapshot!(lowered_code);
            }
        }
    };
}

test_file! { aug_assign, "lowering/aug_assign.fe" }
test_file! { base_tuple, "lowering/base_tuple.fe" }
test_file! { list_expressions, "lowering/list_expressions.fe" }
test_file! { return_unit, "lowering/return_unit.fe" }
test_file! { unit_implicit, "lowering/unit_implicit.fe" }
test_file! { init, "lowering/init.fe" }
test_file! { custom_empty_type, "lowering/custom_empty_type.fe" }
test_file! { nested_tuple, "lowering/nested_tuple.fe" }
test_file! { map_tuple, "lowering/map_tuple.fe" }
test_file! { type_alias_tuple, "lowering/type_alias_tuple.fe" }
test_file! { tuple_destruct, "lowering/tuple_destruct.fe" }
test_file! { module_const, "lowering/module_const.fe" }
test_file! { module_fn, "lowering/module_fn.fe" }
test_file! { struct_fn, "lowering/struct_fn.fe" }
test_file! { ternary, "lowering/ternary.fe" }
test_file! { and_or, "lowering/and_or.fe" }
// TODO: the analyzer rejects lowered nested tuples.
// test_file!(array_tuple, "lowering/array_tuple.fe");
