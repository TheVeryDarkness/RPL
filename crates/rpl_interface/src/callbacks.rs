use rpl_context::PatternCtxt;
// use rpl_middle::ty::RplConfig;
use rustc_interface::interface;
use rustc_middle::ty::TyCtxt;
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
}

impl RplCallbacks {
    pub fn new(rpl_args_var: Option<String>) -> Self {
        Self { rpl_args_var }
    }
}

impl rustc_driver::Callbacks for RplCallbacks {
    // JUSTIFICATION: necessary in RPL driver to set `mir_opt_level`
    #[allow(rustc::bad_opt_access)]
    fn config(&mut self, config: &mut interface::Config) {
        // let previous = config.register_lints.take();
        let rpl_args_var = self.rpl_args_var.take();
        config.psess_created = Some(Box::new(move |psess| {
            track_rpl_args(psess, &rpl_args_var);
            track_files(psess);
        }));
        config.locale_resources = crate::default_locale_resources();

        let previous = config.register_lints.take();
        config.register_lints = Some(Box::new(move |sess, lint_store| {
            // technically we're ~guaranteed that this is none but might as well call anything that
            // is there already. Certainly it can't hurt.
            if let Some(previous) = &previous {
                (previous)(sess, lint_store);
            }

            rpl_patterns::register_lints(lint_store);
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

    fn after_analysis(&mut self, _compiler: &interface::Compiler, tcx: TyCtxt<'_>) -> rustc_driver::Compilation {
        PatternCtxt::entered(|pcx| rpl_driver::check_crate(tcx, pcx));
        /*
        queries.global_ctxt().unwrap().enter(|tcx| {
            let mut lint_store = LaterLintStore::new();
            rpl_driver::register_later_lints(&mut lint_store);
            let side_effect_backtrace = tcx.sess.psess.env_depinfo.lock().iter().any(|&(env, args)| {
                env == Symbol::intern(RPL_ARGS_ENV)
                    && args.is_some_and(|args| {
                        args.as_str()
                            .split_ascii_whitespace()
                            .any(|arg| arg == RplConfig::SIDE_EFFECT_BACKTRACE)
                    })
            });
            create_rpl_ctxt(
                tcx,
                Some(Box::new(lint_store)),
                RplConfig { side_effect_backtrace },
            )
            .enter(|bcx| {
                rpl_later_lint::check_crate(bcx);
            });
        });
        */
        rustc_driver::Compilation::Continue
    }
}
