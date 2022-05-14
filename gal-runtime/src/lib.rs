#![feature(round_char_boundary)]

pub mod config;
pub mod plugin;
pub mod script;

pub use config::*;
pub use gal_script::{Command, Expr, Line, RawValue, Text};
use gal_script::{Loc, ParseError, TextParser};

use plugin::*;
use script::*;
use std::{collections::HashMap, path::Path};
use unicode_width::UnicodeWidthStr;
use wit_bindgen_wasmtime::wasmtime::Store;

pub struct Runtime {
    store: Store<()>,
    modules: HashMap<String, Host>,
}

pub struct Context<'a> {
    pub game: &'a Game,
    pub ctx: RawContext,
    pub res: VarMap,
    runtime: Runtime,
}

impl<'a> Context<'a> {
    fn default_ctx(game: &Game) -> RawContext {
        let mut ctx = RawContext::default();
        ctx.cur_para = game
            .paras
            .first()
            .map(|p| p.tag.clone())
            .unwrap_or_default();
        ctx
    }

    pub fn new(game: &'a Game) -> Self {
        Self::with_context(game, Self::default_ctx(game))
    }

    pub fn with_context(game: &'a Game, ctx: RawContext) -> Self {
        let runtime = load_plugins(&game.plugins, &game.root_path);
        Self {
            game,
            ctx,
            // TODO: load resources
            res: VarMap::default(),
            runtime,
        }
    }

    fn table(&mut self) -> VarTable {
        VarTable::new(&mut self.ctx.locals, &self.res, &mut self.runtime)
    }

    pub fn current_paragraph(&self) -> Option<&'a Paragraph> {
        self.game.find_para(&self.ctx.cur_para)
    }

    pub fn current_text(&self) -> Option<&'a String> {
        self.current_paragraph()
            .and_then(|p| p.actions.get(self.ctx.cur_act))
    }

    pub fn call(&mut self, expr: &impl Callable) -> RawValue {
        self.table().call(expr)
    }

    fn rich_error(&self, text: &str, e: &ParseError) -> String {
        use std::iter::repeat;
        const FREE_LEN: usize = 20;

        let loc = e.loc();
        let loc = Loc(
            text.floor_char_boundary(loc.0),
            text.ceil_char_boundary(loc.1),
        );
        let pre = text.floor_char_boundary(loc.0 - loc.0.min(FREE_LEN));
        let post = text.ceil_char_boundary(loc.1 + (text.len() - loc.1).min(FREE_LEN));

        let para_name = self.current_paragraph().unwrap().title.escape_default();
        let act_num = self.ctx.cur_act + 1;
        let show_code = &text[pre..post];
        let pre_code = &text[pre..loc.0];
        let error_code = &text[loc.0..loc.1];
        format!(
            "Parse error on paragraph \"{para_name}\", act {act_num}:\n    {show_code}\n    {}\n{e}\n",
            repeat(' ')
                .take(UnicodeWidthStr::width_cjk(pre_code))
                .chain(repeat('^').take(UnicodeWidthStr::width_cjk(error_code)))
                .collect::<String>(),
        )
    }

    fn parse_text_rich_error(&self, text: &str) -> Text {
        match TextParser::new(text).parse() {
            Ok(t) => t,
            Err(e) => {
                eprintln!("{}", self.rich_error(text, &e));
                panic!("{}", e);
            }
        }
    }

    fn check_text_rich_error(&self, text: &str) -> bool {
        if let Err(e) = TextParser::new(text).parse() {
            eprintln!("{}", self.rich_error(text, &e));
            false
        } else {
            true
        }
    }

    pub fn next_run(&mut self) -> Option<Text> {
        if let Some(cur_para) = self.current_paragraph() {
            if let Some(act) = self.current_text() {
                self.ctx.cur_act += 1;
                Some(self.parse_text_rich_error(act))
            } else {
                self.ctx.cur_para = cur_para
                    .next
                    .as_ref()
                    .map(|next| {
                        self.call(&self.parse_text_rich_error(next))
                            .get_str()
                            .into()
                    })
                    .unwrap_or_default();
                self.ctx.cur_act = 0;
                self.next_run()
            }
        } else {
            None
        }
    }

    pub fn check(&mut self) -> bool {
        let mut succeed = true;
        for para in &self.game.paras {
            self.ctx.cur_para = para.tag.clone();
            for (index, act) in para.actions.iter().enumerate() {
                self.ctx.cur_act = index;
                succeed &= self.check_text_rich_error(act);
            }
            if let Some(next) = &para.next {
                succeed &= self.check_text_rich_error(next);
            }
        }
        self.ctx = Self::default_ctx(self.game);
        succeed
    }
}
