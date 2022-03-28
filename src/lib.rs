use std::{collections::{HashMap, HashSet}, rc::Rc, cell::{RefCell}};
use enclose::enclose;
use error::Error;
use graph::{ReadOnly, Node, Graph};
use html::Html;
use wasm_bindgen::{prelude::*, JsCast};
use wasm_bindgen_futures::{JsFuture};
use wasm_mutex::Mutex;
use web_sys::{RequestInit, RequestMode, Request, Response, HtmlElement, MouseEvent, Document};

use regex::Regex;

mod html;
mod error;
mod graph;

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

struct State {
    overlay_click_closures: Vec<Closure<dyn FnMut(MouseEvent)>>,
}

impl State {
    pub fn new() -> Self {
        State {
            overlay_click_closures: Default::default(),
        }
    }
}

impl Default for State {
    fn default() -> Self {
        State::new()
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
    let state = Rc::new(Mutex::new(State::new()));

    set_on_click(&html.overlay.div, |event| {
        event.stop_propagation();
    }).forget();

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

    characters.nodes().iter().try_for_each(enclose!((html) move |node| {
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

        set_on_click(&character_el, enclose!((html, state, node) move |_event| {
            wasm_bindgen_futures::spawn_local(enclose!((html, state, node) async move {
                on_character_click(html.clone(), state.clone(), &node).await.unwrap();
            }));
        })).forget();
        html.characters.append_child(&character_el)?;
        Ok::<(), error::Error>(())
    }))?;

    log("complete");
    Ok(())
}

async fn on_character_click(html: Rc<Html>, state: Rc<Mutex<State>>, node: &ReadOnly<Node<Character>>) -> Result<()> {
    let node = node.borrow();
    let character = node.val();

    html.overlay.writing.set_text_content(Some(&character.writing.to_string()));
    html.overlay.readings.set_text_content(Some(&character.readings.join("、")));
    html.overlay.meaning.set_text_content(Some(&character.meaning));

    let document = document()?;

    {
        let mut locked_state = state.lock().await;
        locked_state.overlay_click_closures.clear();

        html.overlay.parents.set_inner_html("");
        node.parents().iter().try_for_each(enclose!((html, state) |p| {
            let closure = add_overlay_relative(&document, html, state, p, true)?;
            locked_state.overlay_click_closures.push(closure);
            Ok::<(), Error>(())
        }))?;

        html.overlay.children.set_inner_html("");
        node.children().iter().try_for_each(enclose!((html, state) |c| {
            let closure = add_overlay_relative(&document, html, state, c, false)?;
            locked_state.overlay_click_closures.push(closure);
            Ok::<(), Error>(())
        }))?;
    }
    
    html.overlay.wrapper.class_list().remove_1("hidden")?;

    Ok(())
}

fn add_overlay_relative(document: &Document, html: Rc<Html>, state: Rc<Mutex<State>>, relative: &ReadOnly<Node<Character>>, is_parent: bool) -> Result<Closure<dyn FnMut(MouseEvent)>> {
    let relative_el = {
        let relative = relative.borrow();
        let el: HtmlElement = document.create_element("div")?.unchecked_into();
        el.set_text_content(Some(&relative.val().writing.to_string()));
        let container = if is_parent {
            &html.overlay.parents
        } else {
            &html.overlay.children
        };
        container.append_child(&el)?;
        el
    };
    let closure = set_on_click(&relative_el, enclose!((html, state, relative) move |_event| {
        wasm_bindgen_futures::spawn_local(enclose!((html, state, relative) async move {
            on_character_click(html.clone(), state.clone(), &relative).await.unwrap();
        }));
    }));
    Ok(closure)
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

            let node = Rc::new(RefCell::new(Node::new(i, Character {
                writing: character,
                is_radical,
                stroke_count,
                meaning,
                readings,
                note,
            })));
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
        node.set_parents(parents);
        node.set_children(children);
    }
    
    Ok(Graph::new(root_nodes))
}