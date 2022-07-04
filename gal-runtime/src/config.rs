use crate::{
    anyhow::{anyhow, Result},
    plugin::Runtime,
};
use gal_locale::Locale;
use gal_script::{
    log::{debug, trace, warn},
    Program, RawValue,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

pub type VarMap = HashMap<String, RawValue>;

#[derive(Debug, Deserialize)]
pub struct Paragraph {
    pub tag: String,
    pub title: Option<String>,
    pub texts: Vec<String>,
    pub next: Option<String>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct RawContext {
    pub cur_para: String,
    pub cur_act: usize,
    pub locals: VarMap,
}

#[derive(Debug, Default, Deserialize)]
struct GameData {
    pub title: String,
    #[serde(default)]
    pub author: String,
    pub paras: HashMap<Locale, Vec<Paragraph>>,
    #[serde(default)]
    pub plugins: PathBuf,
    #[serde(default)]
    pub bgms: PathBuf,
    #[serde(default)]
    pub res: HashMap<Locale, VarMap>,
    pub base_lang: Locale,
}

pub struct Game {
    root_path: PathBuf,
    data: GameData,
    runtime: Runtime,
}

impl Game {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let reader = std::fs::File::open(path.as_ref())?;
        let data: GameData = serde_yaml::from_reader(reader)?;
        let root_path = path
            .as_ref()
            .parent()
            .ok_or_else(|| anyhow!("Cannot get parent from input path."))?;
        let runtime = Runtime::load(&data.plugins, root_path)?;
        Ok(Self {
            data,
            root_path: root_path.to_path_buf(),
            runtime,
        })
    }

    fn choose_from_keys<V>(&self, loc: &Locale, map: &HashMap<Locale, V>) -> Locale {
        let keys = map.keys();
        debug!("Choose \"{}\" from {:?}", loc, keys);
        let res = loc
            .choose_from(keys)
            .unwrap_or_else(|e| {
                warn!("Cannot choose locale: {}", e);
                None
            })
            .unwrap_or_else(|| self.data.base_lang.clone());
        debug!("Chose \"{}\"", res);
        res
    }

    pub fn runtime(&self) -> &Runtime {
        &self.runtime
    }

    pub fn title(&self) -> &str {
        &self.data.title
    }

    pub fn author(&self) -> &str {
        &self.data.author
    }

    pub fn paras(&self) -> &HashMap<Locale, Vec<Paragraph>> {
        &self.data.paras
    }

    pub fn resources(&self) -> &HashMap<Locale, VarMap> {
        &self.data.res
    }

    pub fn base_lang(&self) -> &Locale {
        &self.data.base_lang
    }

    pub fn plugin_dir(&self) -> PathBuf {
        self.root_path.join(&self.data.plugins)
    }

    pub fn bgm_dir(&self) -> PathBuf {
        self.root_path.join(&self.data.bgms)
    }

    pub fn find_para(&self, loc: &Locale, tag: &str) -> Option<&Paragraph> {
        if let Some(paras) = self
            .data
            .paras
            .get(&self.choose_from_keys(loc, &self.data.paras))
        {
            for p in paras {
                if p.tag == tag {
                    return Some(p);
                }
            }
        }
        None
    }

    pub fn find_para_fallback(&self, loc: &Locale, tag: &str) -> Fallback<Paragraph> {
        Fallback::new(
            self.find_para(loc, tag),
            self.find_para(&self.data.base_lang, tag),
        )
    }

    pub fn find_res(&self, loc: &Locale) -> Option<&HashMap<String, RawValue>> {
        self.data
            .res
            .get(&self.choose_from_keys(loc, &self.data.res))
    }

    pub fn find_res_fallback(&self, loc: &Locale) -> Fallback<HashMap<String, RawValue>> {
        Fallback::new(self.find_res(loc), self.find_res(&self.data.base_lang))
    }
}

pub struct Fallback<'a, T> {
    data: Option<&'a T>,
    base_data: Option<&'a T>,
}

impl<'a, T> Fallback<'a, T> {
    pub(crate) fn new(data: Option<&'a T>, base_data: Option<&'a T>) -> Self {
        Self { data, base_data }
    }

    pub fn is_some(&self) -> bool {
        self.data.is_some() || self.base_data.is_some()
    }

    pub fn and_then<V>(&self, mut f: impl FnMut(&'a T) -> Option<V>) -> Option<V> {
        self.data.and_then(|t| f(t)).or_else(|| {
            trace!("Fallback occurred");
            self.base_data.and_then(|t| f(t))
        })
    }
}

#[derive(Debug, Default)]
pub struct Action {
    pub data: ActionData,
    pub switch_actions: Vec<Program>,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct ActionData {
    pub line: String,
    pub character: Option<String>,
    pub switches: Vec<Switch>,
    pub bgm: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Switch {
    pub text: String,
    pub enabled: bool,
}
