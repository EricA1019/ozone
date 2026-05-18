use crate::cli::args::*;

pub fn edit_message(args: EditArgs) -> Result<(), String> {
    let mut engine = crate::cli::util::open_cli_engine()?;
    let message = engine.edit(args)?;

    println!("Edited message.");
    crate::cli::print::print_message(&message);
    Ok(())
}