use std::{collections::{HashMap, HashSet}, rc::Rc, cell::{RefCell}, ops::Deref, cmp::Ordering};
use enclose::enclose;
use error::Error;
use html::Html;
use wasm_bindgen::{prelude::*, JsCast};
use wasm_bindgen_futures::JsFuture;
use web_sys::{RequestInit, RequestMode, Request, Response, HtmlElement, MouseEvent, Document};
use std::hash::Hash;

use regex::Regex;

mod html;
mod error;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    pub fn error(s: &str);
    #[wasm_bindgen(js_namespace = console)]
    pub fn log(s: &str);
    #[wasm_bindgen(js_namespace = console)]
    pub fn warn(s: &str);
}

pub type Result<T> = std::result::Result<T, error::Error>;

type NodeId = usize;

const WRITING_RE: &str = r"^([^ ]+).+$";
const WRITING_READING_RE: &str = r"^([^ ]+) \[([^\[\]]+)\].+$";

#[derive(Debug, Clone)]
pub struct Character {
    writing: char,
    is_radical: bool,
    stroke_count: u8,
    meaning: String,
    readings: Vec<String>,
    note: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ReadOnly<T> {
    inner: Rc<RefCell<T>>
}

impl<T> ReadOnly<T> {
    pub fn borrow<'a>(&'a self) -> impl Deref<Target = T> + 'a {
        self.inner.deref().borrow()
    }
}

impl <T> From <Rc<RefCell<T>>> for ReadOnly<T> {
    fn from(inner: Rc<RefCell<T>>) -> Self {
        ReadOnly { inner }
    }
}


#[derive(Debug, Clone)]
pub struct Graph<T> {
    children: Vec<Rc<RefCell<Node<T>>>>,
}

impl <T> Graph<T> {
    pub fn sort_by(&mut self, handler: impl Fn(&ReadOnly<Node<T>>, &ReadOnly<Node<T>>) -> Ordering) {
        let mut order: Vec<Rc<RefCell<Node<T>>>> = Default::default();
        let mut learnable_nodes = self.children.clone();
        let mut parents_learned: HashMap<NodeId, usize> = Default::default();
        while !learnable_nodes.is_empty() {
            learnable_nodes.sort_by(|a, b| {
                handler(&a.clone().into(), &b.clone().into())
            });
            learnable_nodes.reverse();
            let next = learnable_nodes.pop().unwrap();
            {
                let next = next.deref().borrow();
                for child in &next.children {
                    let learnable = {
                        let child = child.deref().borrow();
                        let count = parents_learned.entry(child.id).or_insert(0);
                        *count += 1;
                        *count == child.parents.len()
                    };
                    if learnable {
                        learnable_nodes.push(child.clone());
                    }
                }
            }
            order.push(next);
        }
        self.children = order;
    }

    pub fn nodes(&self) -> Vec<ReadOnly<Node<T>>> {
        self.children.iter().map(|c| ReadOnly::from(c.clone())).collect::<Vec<_>>()
    }
}

#[derive(Debug, Clone)]
pub struct Node<T> {
    id: NodeId,
    val: T,
    parents: Vec<Rc<RefCell<Node<T>>>>,
    children: Vec<Rc<RefCell<Node<T>>>>,
}

impl <T> Node<T> {
    pub fn val(&self) -> &T {
        &self.val
    }

    pub fn children(&self) -> Vec<ReadOnly<Node<T>>> {
        self.children.iter().map(|c| ReadOnly::from(c.clone())).collect::<Vec<_>>()
    }

    pub fn parents(&self) -> Vec<ReadOnly<Node<T>>> {
        self.parents.iter().map(|c| ReadOnly::from(c.clone())).collect::<Vec<_>>()
    }

    pub fn descendent_count(&self) -> usize {
        let children = &self.children;
        let mut len = children.len();
        for child in children {
            len += child.deref().borrow().descendent_count()
        }
        len
    }

    pub fn ancestor_count(&self) -> usize {
        let parents = &self.parents;
        let mut len = parents.len();
        for parent in parents {
            len += parent.deref().borrow().ancestor_count()
        }
        len
    }
}

async fn fetch_local_text(path: &str) -> Result<String> {
    let location = web_sys::window().expect("no window")
        .location();

    let mut opts = RequestInit::new();
    opts.method("GET");
    opts.mode(RequestMode::Cors);

    let url = format!("{}{}{}", location.origin()?, location.pathname()?, path);
    let request = Request::new_with_str_and_init(&url, &opts)?;

    let window = web_sys::window().expect("window not found");
    let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
    let resp: Response = resp_value.dyn_into()?;
    let text = JsFuture::from(resp.text()?).await?;
    let text = text.as_string().expect("no string found");
    Ok(text)
}

#[wasm_bindgen(start)]
pub async fn main() -> std::result::Result<(), JsValue> {
    console_error_panic_hook::set_once();
    init().await.unwrap();
    Ok(())
}

pub fn document() -> Result<Document> {
    Ok(web_sys::window()
        .ok_or(Error::WindowNotFound)?
        .document()
        .ok_or(Error::DocumentNotFound)?
    )
}

async fn init() -> Result<()> {
    let document = crate::document()?;
    let html = Rc::new(Html::new(&document)?);

    set_on_click(&html.overlay.wrapper, enclose!((html) move |_event| {
        html.overlay.wrapper.class_list().add_1("hidden").unwrap();
    })).forget();

    let characters = fetch_local_text("/data/characters.txt").await?;
    let mut characters = parse_characters(&characters)?;

    log("loading terms...");
    let terms = fetch_local_text("/data/edict2u.txt").await?;
    log("parsing terms...");
    let terms = parse_terms(&terms)?;

    log("grouping terms...");
    let grouped_terms = group_terms_by_chars(&terms);

    log("sorting characters");
    characters.sort_by(|a, b| {
        let a_score = grouped_terms.get(&a.borrow().val().writing).map(|terms| terms.iter().filter(|t| t.popular).count()).unwrap_or(0);
        let b_score = grouped_terms.get(&b.borrow().val().writing).map(|terms| terms.iter().filter(|t| t.popular).count()).unwrap_or(0);
        b_score.cmp(&a_score)
    });

    characters.nodes().iter().try_for_each(|node| {
        let character_el = {
            let node = node.borrow();
            let val = node.val();
            let character_el: HtmlElement = document.create_element("div")?.unchecked_into();
            character_el.class_list().add_1("character")?;
            if val.is_radical {
                character_el.class_list().add_1("radical")?;
            } else {
                character_el.class_list().add_1("kanji")?;
            }
            character_el.set_text_content(Some(&val.writing.to_string()));
            character_el
        };

        set_on_click(&character_el, enclose!((html, node) move |_event| {
            on_character_click(&html, &node).unwrap();
        })).forget();
        html.characters.append_child(&character_el)?;
        Ok::<(), error::Error>(())
    })?;

    log("complete");
    Ok(())
}

fn on_character_click(html: &Html, node: &ReadOnly<Node<Character>>) -> Result<()> {
    let node = node.borrow();
    let character = node.val();

    html.overlay.writing.set_text_content(Some(&character.writing.to_string()));
    html.overlay.readings.set_text_content(Some(&character.readings.join("、")));
    html.overlay.meaning.set_text_content(Some(&character.meaning));

    let parents_str = node.parents().iter().map(|p| p.borrow().val().writing.to_string()).collect::<Vec<_>>().join("");
    let children_str = node.children().iter().map(|c| c.borrow().val().writing.to_string()).collect::<Vec<_>>().join("");

    html.overlay.parents.set_text_content(Some(&parents_str));
    html.overlay.children.set_text_content(Some(&children_str));

    html.overlay.wrapper.class_list().remove_1("hidden")?;

    Ok(())
}

fn set_on_click<F>(el: &HtmlElement, handler: F) -> Closure<dyn FnMut(MouseEvent)>
    where
        F: FnMut(MouseEvent) + 'static {
            let c = Closure::wrap(Box::new(handler) as Box<dyn FnMut(MouseEvent)>);
            el.set_onclick(Some(c.as_ref().unchecked_ref()));
            c
        }

#[derive(Debug, Clone)]
pub struct Term {
    pub id: String,
    pub writings: Vec<String>,
    pub readings: Option<Vec<String>>,
    pub meanings: Vec<String>,
    pub popular: bool,
}

fn group_terms_by_chars(terms: &[Term]) -> HashMap<char, Vec<&Term>> {
    terms.iter().fold(HashMap::new(), |mut acc, term| {
        let chars = term.writings.iter().fold(HashSet::new(), |mut acc, writing| {
            writing.chars().for_each(|c| {
                acc.insert(c);
            });
            acc
        });

        chars.iter().for_each(|c| {
            let entry = acc.entry(*c).or_insert(Vec::new());
            entry.push(term);
        });
        acc
    })
}

fn parse_terms(s: &str) -> Result<Vec<Term>> {
    let lines = s.split("\n").map(|s| s.trim());
    let terms: Result<Vec<Term>> = lines.enumerate().try_fold(Vec::new(),|mut acc, (i, line)| {
        if i > 0 && !line.is_empty() {
            let term = parse_term(line)?;
            acc.push(term);
        }
        Ok(acc)
    });
    terms
}

fn parse_term(s: &str) -> Result<Term> {
    let fields = s.split("/").filter(|s| !s.is_empty()).collect::<Vec<_>>();
    let id = fields[fields.len()-1].to_string();
    let mut popular = false;
    let meanings = fields[1..fields.len()-1].iter().filter_map(|s| {
        if s.eq_ignore_ascii_case("(P)") {
            popular = true;
            None
        } else {
            Some(s.to_string())
        }
    }).collect();
    let (writings, readings) = if let Some(cap) = Regex::new(WRITING_READING_RE).unwrap().captures(fields[0]) {
        let writings = cap[1].split(";").map(|s| s.trim().to_string()).collect();
        let readings = Some(cap[2].split(";").map(|s| s.trim().to_string()).collect());
        (writings, readings)
    } else if let Some(cap) = Regex::new(WRITING_RE).unwrap().captures(fields[0]) {
        let writings = cap[1].split(";").map(|s| s.trim().to_string()).collect();
        (writings, None)
    } else {
        panic!("unknown entry: {}", s);
    };
    Ok(Term {
        id,
        popular,
        writings,
        readings,
        meanings,
    })
}

fn parse_characters(s: &str) -> Result<Graph<Character>> {
    let mut nodes: HashMap<char, Rc<RefCell<Node<Character>>>> = Default::default();
    let mut parents: HashMap<char, Vec<char>> = Default::default();
    let mut children: HashMap<char, Vec<char>> = Default::default();
    s
        .split("\n")
        .enumerate()
        .for_each(|(i, l)| {
            let fields = l.split("\t").collect::<Vec<_>>();
            let character = fields[0].chars().next().unwrap();
            let p = fields[1]
                .split("")
                .filter_map(|s| s.chars().next())
                .collect::<Vec<_>>();
            let stroke_count = fields[2].parse().expect("failed parsing stroke count");
            let readings = fields[3].split("、").map(|s| s.trim().to_string()).collect();
            let meaning = fields[4].to_string();
            let is_radical = fields[5].eq_ignore_ascii_case("1");
            let note = if fields[6].is_empty() {
                None
            } else {
                Some(fields[6].to_string())
            };

            let node = Rc::new(RefCell::new(Node {
                id: i,
                val: Character {
                    writing: character,
                    is_radical,
                    stroke_count,
                    meaning,
                    readings,
                    note,
                },
                parents: Default::default(),
                children: Default::default(),
            }));
            nodes.insert(character, node);

            for p in &p {
                let children = children.entry(*p).or_insert(Default::default());
                children.push(character);
            }

            parents.insert(character, p);
        });

    let mut root_nodes: Vec<Rc<RefCell<Node<Character>>>> = Default::default();

    for (k, node) in nodes.iter() {
        let parents = parents.get(k).unwrap().iter().map(|p| nodes.get(p).unwrap().clone()).collect::<Vec<_>>();
        let children = children.get(k).unwrap_or(&vec![]).iter().map(|c| nodes.get(c).unwrap().clone()).collect::<Vec<_>>();

        if parents.is_empty() {
            root_nodes.push(node.clone());
        }

        let mut node = node.borrow_mut();
        node.parents = parents;
        node.children = children;
    }
    
    Ok(Graph {
        children: root_nodes,
    })
}