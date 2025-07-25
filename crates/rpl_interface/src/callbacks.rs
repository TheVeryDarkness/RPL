use std::path::PathBuf;
use std::sync::OnceLock;

use rpl_context::PatternCtxt;
use rpl_driver::{ERROR_FOUND, ErrorFound};
#[cfg(feature = "timing")]
use rpl_driver::{TIMING, Timing};
use rpl_meta::cli::collect_file_from_string_args;
// use rpl_middle::ty::RplConfig;
use rustc_interface::interface;
use rustc_middle::ty::TyCtxt;
use rustc_session::EarlyDiagCtxt;
use rustc_session::parse::ParseSess;
use rustc_span::Symbol;

// use crate::passes::create_rpl_ctxt;

pub static RPL_ARGS_ENV: &str = "RPL_ARGS";

fn track_rpl_args(psess: &mut ParseSess, args_env_var: &Option<String>) {
    psess.env_depinfo.get_mut().insert((
        Symbol::intern(RPL_ARGS_ENV),
        args_env_var.as_deref().map(Symbol::intern),
    ));
}

#[cfg_attr(not(debug_assertions), allow(unused_variables))]
/// Track files that may be accessed at runtime in `file_depinfo` so that cargo will re-run RPL
/// when any of them are modified
fn track_files(psess: &mut ParseSess) {
    let file_depinfo = psess.file_depinfo.get_mut();

    // During development track the `rpl-driver` executable so that cargo will re-run RPL
    // whenever it is rebuilt
    #[cfg(debug_assertions)]
    if let Ok(current_exe) = std::env::current_exe()
        && let Some(current_exe) = current_exe.to_str()
    {
        file_depinfo.insert(Symbol::intern(current_exe));
    }
}

/// This is different from `DefaultCallbacks` that it will inform Cargo to track the value of
/// `RPL_ARGS` environment variable.
pub struct RustcCallbacks {
    rpl_args_var: Option<String>,
}

impl RustcCallbacks {
    pub fn new(rpl_args_var: Option<String>) -> Self {
        Self { rpl_args_var }
    }
}

impl rustc_driver::Callbacks for RustcCallbacks {
    fn config(&mut self, config: &mut interface::Config) {
        let rpl_args_var = self.rpl_args_var.take();
        config.psess_created = Some(Box::new(move |psess| {
            track_rpl_args(psess, &rpl_args_var);
        }));
    }
}

pub struct DefaultCallbacks;
impl rustc_driver::Callbacks for DefaultCallbacks {}

pub struct RplCallbacks {
    rpl_args_var: Option<String>,
    pattern_paths: Vec<String>,
}

impl RplCallbacks {
    pub fn new(rpl_args_var: Option<String>, pattern_paths: Vec<String>) -> Self {
        Self {
            rpl_args_var,
            pattern_paths,
        }
    }
}

/// Arena for [`MetaContext`] to use, initialized lazily.
/// This is used to avoid having to pass the arena around everywhere.
/// It is initialized in [`after_analysis`] of [`RplCallbacks`].
/// The arena is used to allocate the [`MetaContext`] and its data structures.
/// It is also used to allocate the `patterns_and_paths` in [`after_analysis`].
/// This is necessary because [`MetaContext`] needs to be allocated in a single arena
/// to avoid lifetime issues with the data structures it contains.
/// The `patterns_and_paths` are allocated in the same arena to ensure that they
/// have the same lifetime as the [`MetaContext`].
///
/// [`after_analysis`]: rustc_driver::Callbacks::after_analysis
/// [`MetaContext`]: rpl_meta::context::MetaContext
static MCTX_ARENA: OnceLock<rpl_meta::arena::Arena<'_>> = OnceLock::new();
/// The [`MetaContext`] for RPL, initialized lazily.
///
/// [`MetaContext`]: rpl_meta::context::MetaContext
static MCTX: OnceLock<rpl_meta::context::MetaContext<'_>> = OnceLock::new();
static PATTERNS: OnceLock<Vec<(PathBuf, String)>> = OnceLock::new();

impl rustc_driver::Callbacks for RplCallbacks {
    // JUSTIFICATION: necessary in RPL driver to set `mir_opt_level`
    #[allow(rustc::bad_opt_access)]
    fn config(&mut self, config: &mut interface::Config) {
        let rpl_args_var = self.rpl_args_var.take();
        config.psess_created = Some(Box::new(move |psess| {
            track_rpl_args(psess, &rpl_args_var);
            track_files(psess);
        }));
        config.locale_resources = crate::default_locale_resources();

        let mctx_arena = MCTX_ARENA.get_or_init(rpl_meta::arena::Arena::default);
        let patterns_and_paths = PATTERNS.get_or_init(|| {
            collect_file_from_string_args(&self.pattern_paths, || {
                EarlyDiagCtxt::new(config.opts.error_format).early_fatal(ErrorFound)
            })
        });
        // let dcx = compiler.sess.dcx();
        let mut error_counter = 0;
        let mctx = MCTX.get_or_init(|| {
            let mctx = rpl_meta::parse_and_collect(mctx_arena, patterns_and_paths, |error| {
                error_counter += 1;
                eprintln!("{error_counter}. {error}"); //FIXME: this would mess up when running on a workspace with multiple crates
                // let _ = dcx.emit_err(error.clone());
            });

            let mut mctx = mctx;
            mctx.add_lint(ERROR_FOUND);
            #[cfg(feature = "timing")]
            mctx.add_lint(TIMING);

            mctx
        });
        // dcx.abort_if_errors();
        if error_counter > 0 {
            EarlyDiagCtxt::new(config.opts.error_format).early_fatal(ErrorFound);
        }

        let previous = config.register_lints.take();
        config.register_lints = Some(Box::new(move |sess, lint_store| {
            // technically we're ~guaranteed that this is none but might as well call anything that
            // is there already. Certainly it can't hurt.
            if let Some(previous) = &previous {
                (previous)(sess, lint_store);
            }

            //FIXME: consider collect patterns earlier, so that we can register lints here
            mctx.register_lints(lint_store);
        }));

        config.override_queries = Some(|_sess, providers| {
            rpl_driver::provide(providers);
        });

        // Disable `debug_assertions` in order not to affect the side effects detection,
        // as we don't consider `debug_assertions` to be side effects.
        config.opts.debug_assertions = false;

        // // FIXME: #4825; This is required, because RPL lints that are based on MIR have to be
        // // run on the unoptimized MIR. On the other hand this results in some false negatives. If
        // // MIR passes can be enabled / disabled separately, we should figure out, what passes to
        // // use for RPL.

        // Disable optimizations because it can affect undefined behaviors.
        _ = config.opts.unstable_opts.mir_opt_level.get_or_insert(1);

        // We rely on `-Z inline-mir` to get the inlined MIR.
        if *config.opts.unstable_opts.inline_mir.get_or_insert(true) {
            // use rustc_session::config::InliningThreshold;
            // _ = config.opts.unstable_opts.inline_mir_threshold.get_or_insert(100);
            // _ = config.opts.unstable_opts.cross_crate_inline_threshold =
            // InliningThreshold::Always; _ = config.opts.unstable_opts.
            // inline_mir_hint_threshold.get_or_insert(100); _ = config
            //     .opts
            //     .unstable_opts
            //     .inline_mir_forwarder_threshold
            //     .get_or_insert(50);
        }

        // Disable flattening and inlining of format_args!(), so the HIR matches with the AST.
        config.opts.unstable_opts.flatten_format_args = false;
    }
    fn after_analysis(&mut self, compiler: &interface::Compiler, tcx: TyCtxt<'_>) -> rustc_driver::Compilation {
        #[cfg(feature = "timing")]
        let start = std::time::Instant::now();

        let mctx_arena = MCTX_ARENA.get_or_init(rpl_meta::arena::Arena::default);
        let patterns_and_paths = PATTERNS
            .get_or_init(|| collect_file_from_string_args(&self.pattern_paths, || tcx.dcx().emit_fatal(ErrorFound)));

        // let dcx = compiler.sess.dcx();
        let mut error_counter = 0;
        let mctx = MCTX.get_or_init(|| {
            rpl_meta::parse_and_collect(mctx_arena, patterns_and_paths, |error| {
                error_counter += 1;
                eprintln!("{error_counter}. {error}"); //FIXME: this would mess up when running on a workspace with multiple crates
                // let _ = dcx.emit_err(error.clone());
            })
        });
        // dcx.abort_if_errors();
        if error_counter > 0 {
            tcx.dcx().emit_fatal(ErrorFound);
        }

        #[cfg(feature = "timing")]
        {
            use rustc_hir::def_id::CrateNum;

            let time = start.elapsed().as_nanos().try_into().unwrap();
            let hir_id = rustc_hir::hir_id::CRATE_HIR_ID;
            let crate_name = tcx.crate_name(CrateNum::ZERO);
            tcx.emit_node_span_lint(
                TIMING,
                hir_id,
                tcx.hir().span(hir_id),
                Timing {
                    time,
                    stage: "parse_and_collect",
                    crate_name,
                },
            );
        }
        compiler.sess.time("check_crate", || {
            PatternCtxt::entered(|pcx| rpl_driver::check_crate(tcx, pcx, mctx));
        });

        rustc_driver::Compilation::Continue
    }
}
