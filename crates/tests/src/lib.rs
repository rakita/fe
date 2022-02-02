
#[cfg(test)]
mod crashes;
#[cfg(test)]
mod demo_erc20;
#[cfg(test)]
mod demo_guestbook;
#[cfg(test)]
mod demo_uniswap;
#[cfg(test)]
mod differential;
#[cfg(test)]
mod features;
#[cfg(test)]
mod ingots;
#[cfg(test)]
mod runtime;
#[cfg(test)]
mod solidity;
#[cfg(test)]
mod stress;

#[cfg(test)]
pub mod test_prebuilds {
    use fevm::{Contract, ContractBuilder, Fevm};
    use once_cell::sync::Lazy;
    use std::sync::Arc;
    pub static DIFF_VM: Lazy<Fevm<'static>> = Lazy::new(|| {
        Fevm::new()
    });
  
    pub static DIFF_CONTRACTS: Lazy<Vec<(Contract<'static>, Contract<'static>)>> = Lazy::new(|| {
        let differential_fe = vec!["math_i8", "math_u8", "storage_and_memory"];
        differential_fe.into_iter().map(|name| {
            let fe_contract = ContractBuilder::new(&DIFF_VM)
            .fixture(format!("differential/{}.fe", name).as_str(), format!("Foo{}Fe", &name).as_str());
            let sol_contract = ContractBuilder::new(&DIFF_VM)
                .sol_fixture(format!("differential/{}.sol", name).as_str(), format!("Foo{}Sol", &name).as_str());
            (fe_contract, sol_contract)
        }).collect::<Vec<_>>()
    });

    
}

#[cfg(test)]
pub use test_prebuilds::*;
