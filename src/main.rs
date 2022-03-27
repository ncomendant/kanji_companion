use std::{collections::{HashMap, HashSet}, rc::Rc, cell::{RefCell}, ops::Deref, cmp::Ordering, io::{BufReader, BufRead}};
use std::hash::Hash;

use regex::Regex;

mod error;

pub type Result<T> = std::result::Result<T, error::Error>;

type NodeId = usize;

const WRITING_RE: &str = r"^([^ ]+).+$";
const WRITING_READING_RE: &str = r"^([^ ]+) \[([^\[\]]+)\].+$";

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


pub struct Graph<T> {
    children: Vec<Rc<RefCell<Node<T>>>>,
}

impl <T: Eq + Hash> Graph<T> {
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

#[derive(Debug)]
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

    pub fn descendent_len(&self) -> usize {
        let children = &self.children;
        let mut len = children.len();
        for child in children {
            len += child.deref().borrow().descendent_len()
        }
        len
    }

    pub fn ancestor_len(&self) -> usize {
        let parents = &self.parents;
        let mut len = parents.len();
        for parent in parents {
            len += parent.deref().borrow().ancestor_len()
        }
        len
    }
}

fn main() {
    println!("parsing characters...");
    let mut characters = parse_characters().unwrap();
    
    println!("parsing terms...");
    let terms = parse_terms().unwrap();

    println!("grouping terms...");
    let grouped_terms = group_terms_by_chars(&terms);

    println!("sorting characters");
    characters.sort_by(|a, b| {
        let a_terms = grouped_terms.get(a.borrow().val());
        let b_terms = grouped_terms.get(b.borrow().val());

        let a_score = if let Some(a_terms) = a_terms {
            a_terms.len()
        } else {
            0
        };

        let b_score = if let Some(b_terms) = b_terms {
            b_terms.len()
        } else {
            0
        };

        b_score.cmp(&a_score)
    });

    println!("{:?}", characters.nodes().iter().map(|n| *n.deref().borrow().val()).collect::<Vec<_>>());

    println!("complete");
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

fn parse_terms() -> Result<Vec<Term>> {
    let file = std::fs::File::open("data/edict2u")?;
    let reader = BufReader::new(file);
    let terms: Result<Vec<Term>> = reader.lines().enumerate().try_fold(Vec::new(),|mut acc, (i, l)| {
        if i > 0 {
            let term = parse_term(&l?)?;
            acc.push(term);
        }
        Ok(acc)
    });
    Ok(terms?)
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
        let writings = cap[1].split(";").map(|s| s.to_string()).collect();
        let readings = Some(cap[2].split(";").map(|s| s.to_string()).collect());
        (writings, readings)
    } else if let Some(cap) = Regex::new(WRITING_RE).unwrap().captures(fields[0]) {
        let writings = cap[1].split(";").map(|s| s.to_string()).collect();
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

fn parse_characters() -> Result<Graph<char>> {
    let mut nodes: HashMap<char, Rc<RefCell<Node<char>>>> = Default::default();
    let mut parents: HashMap<char, Vec<char>> = Default::default();
    let mut children: HashMap<char, Vec<char>> = Default::default();
    std::fs::read_to_string("data/characters")?
        .split("\n")
        .enumerate()
        .for_each(|(i, l)| {
            let fields = l.split("\t").collect::<Vec<_>>();
            let character = fields[0].chars().next().unwrap();
            let p = fields[1]
                .split("")
                .filter_map(|s| s.chars().next())
                .collect::<Vec<_>>();

            let node = Rc::new(RefCell::new(Node {
                id: i,
                val: character,
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

    let mut root_nodes: Vec<Rc<RefCell<Node<char>>>> = Default::default();

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