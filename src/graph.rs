use std::{ops::Deref, cell::RefCell, rc::Rc, cmp::Ordering, collections::HashMap};

type NodeId = usize;

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
    pub fn new(children: Vec<Rc<RefCell<Node<T>>>>) -> Self {
        Graph {
            children,
        }
    }

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
    pub fn new(id: NodeId, val: T) -> Self {
        Node {
            id,
            val,
            parents: Default::default(),
            children: Default::default(),
        }
    }

    pub fn val(&self) -> &T {
        &self.val
    }

    pub fn children(&self) -> Vec<ReadOnly<Node<T>>> {
        self.children.iter().map(|c| ReadOnly::from(c.clone())).collect::<Vec<_>>()
    }

    pub fn parents(&self) -> Vec<ReadOnly<Node<T>>> {
        self.parents.iter().map(|c| ReadOnly::from(c.clone())).collect::<Vec<_>>()
    }

    pub fn set_children(&mut self, children: Vec<Rc<RefCell<Node<T>>>>) {
        self.children = children;
    }

    pub fn set_parents(&mut self, parents: Vec<Rc<RefCell<Node<T>>>>) {
        self.parents = parents;
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