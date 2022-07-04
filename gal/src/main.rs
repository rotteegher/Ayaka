use clap::Parser;
use gal_runtime::{
    anyhow::{bail, Result},
    Context, Game, Locale,
};
use std::{
    ffi::OsString,
    io::{stdin, stdout, Write},
    sync::Arc,
};

#[derive(Debug, Parser)]
#[clap(about, version, author)]
pub struct Options {
    input: OsString,
    #[clap(long)]
    check: bool,
    #[clap(long)]
    auto: bool,
}

fn read_line() -> Result<String> {
    stdout().flush()?;
    let mut s = String::new();
    stdin().read_line(&mut s)?;
    Ok(s)
}

fn pause(auto: bool) -> Result<()> {
    if auto {
        println!();
    } else {
        read_line()?;
    }
    Ok(())
}

fn main() -> Result<()> {
    let opts = Options::parse();
    env_logger::try_init()?;
    let game = Arc::new(Game::open(&opts.input)?);
    let mut ctx = Context::new(game, Locale::current())?;
    if opts.check {
        if !ctx.check() {
            bail!("Check failed.");
        }
    }
    while let Some(action) = ctx.next_run() {
        if let Some(name) = &action.data.character {
            print!("_{}_", name);
        }
        print!("{}", action.data.line);
        if !action.data.switches.is_empty() {
            for (i, s) in action.data.switches.iter().enumerate() {
                if s.enabled {
                    print!("\n-{}- {}", i + 1, s.text);
                } else {
                    print!("\n-x- {}", s.text);
                }
            }
            println!();
            loop {
                let s = read_line()?;
                if let Ok(i) = s.trim().parse::<usize>() {
                    let valid = i > 0 && i <= action.switch_actions.len();
                    if valid {
                        ctx.call(&action.switch_actions[i - 1]);
                        break;
                    }
                }
                println!("Invalid switch, enter again!");
            }
        } else {
            pause(opts.auto)?;
        }
    }
    Ok(())
}
