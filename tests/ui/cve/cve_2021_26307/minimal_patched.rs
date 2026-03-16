//@only-target: x86_64

/// Uses Rust's `cpuid` function from the `arch` module.
pub mod native_cpuid {
    use self::arch::CpuidResult as CpuIdResult;

    #[cfg(all(target_arch = "x86", not(target_env = "sgx"), target_feature = "sse"))]
    use core::arch::x86 as arch;
    #[cfg(all(target_arch = "x86_64", not(target_env = "sgx")))]
    use core::arch::x86_64 as arch;

    pub fn cpuid_count(a: u32, c: u32) -> CpuIdResult {
        // Safety: CPUID is supported on all x86_64 CPUs and all x86 CPUs with
        // SSE, but not by SGX.
        let result = unsafe { self::arch::__cpuid_count(a, c) };

        CpuIdResult {
            eax: result.eax,
            ebx: result.ebx,
            ecx: result.ecx,
            edx: result.edx,
        }
    }
}

/// Macro which queries cpuid directly.
///
/// First parameter is cpuid leaf (EAX register value),
/// second optional parameter is the subleaf (ECX register value).
#[macro_export]
macro_rules! cpuid {
    ($eax:expr) => {
        $crate::native_cpuid::cpuid_count($eax as u32, 0)
    };

    ($eax:expr, $ecx:expr) => {
        $crate::native_cpuid::cpuid_count($eax as u32, $ecx as u32)
    };
}
/// Main type used to query for information about the CPU we're running on.
#[derive(Debug, Default)]
pub struct CpuId {
    max_eax_value: u32,
}

const EAX_VENDOR_INFO: u32 = 0x0;

/// Return new CPUID struct.
pub fn new() -> CpuId {
    let res = cpuid!(EAX_VENDOR_INFO);
    CpuId {
        max_eax_value: res.eax,
    }
}
