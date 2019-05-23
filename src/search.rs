extern crate stopwatch;
use std::cell::RefCell;
use std::rc::{Rc, Weak};
use stopwatch::Stopwatch;
use crate::world::State;
use crate::world::Cell;
use crate::world::World;

// 搜索时除了世界本身的状态，还需要记录别的一些信息。
pub struct Search<W: World> {
    world: W,
    // 存放在搜索过程中设定了值的细胞
    set_table: Vec<Weak<RefCell<Cell>>>,
    // 下一个要检验其状态的细胞，详见 proceed 函数
    next_set: usize,
    // 是否计时
    time: bool,
    // 记录搜索时间
    stopwatch: Stopwatch,
}

impl<W: World> Search<W> {
    pub fn new(world: W, time: bool) -> Search<W> {
        let set_table = Vec::with_capacity(world.size());
        let stopwatch = Stopwatch::new();
        Search {world, set_table, next_set: 0, time, stopwatch}
    }

    // 只有细胞原本的状态为未知时才改变细胞的状态；若原本的状态和新的状态矛盾则返回 false
    // 并且把细胞记录到 set_table 中
    fn set_cell(&mut self, cell: Rc<RefCell<Cell>>, state: State) -> Result<(), ()> {
        if let Some(old_state) = cell.borrow().state {
            if state == old_state {
                return Ok(());
            } else {
                return Err(());
            }
        };
        cell.borrow_mut().state = Some(state);
        cell.borrow_mut().free = false;
        self.set_table.push(Rc::downgrade(&cell));
        Ok(())
    }

    // 确保由一个细胞前一代的邻域能得到这一代的状态；若不能则返回 false
    fn consistify(&mut self, cell: Rc<RefCell<Cell>>) -> Result<(), ()> {
        let pred = cell.borrow().pred.upgrade().unwrap();
        let desc = W::get_desc(&pred.borrow());
        if let Some(state) = W::transition(&desc) {
            self.set_cell(cell.clone(), state)?;
        }
        if let Some(state) = cell.borrow().state {
            if let Some(state) = W::implication(&desc, state) {
                self.set_cell(pred.clone(), state)?;
            }
            if let Some(state) = W::implication_nbhd(&desc, state) {
                for neigh in &pred.borrow().nbhd {
                    if let Some(neigh) = neigh.upgrade() {
                        if neigh.borrow().state.is_none() {
                            self.set_cell(neigh, state)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    // consistify 一个细胞本身，后一代，以及后一代的邻域中的所有细胞
    fn consistify10(&mut self, cell: Rc<RefCell<Cell>>) -> Result<(), ()> {
        let succ = cell.borrow().succ.upgrade().unwrap();
        self.consistify(cell)?;
        self.consistify(succ.clone())?;
        for neigh in &succ.borrow().nbhd {
            if let Some(neigh) = neigh.upgrade() {
                self.consistify(neigh)?;
            }
        }
        Ok(())
    }


    // 通过 consistify 和对称性把所有能确定的细胞确定下来
    fn proceed(&mut self) -> Result<(), ()> {
        while self.next_set < self.set_table.len() {
            let cell = self.set_table[self.next_set].upgrade().unwrap();
            let state = cell.borrow().state.unwrap();
            for sym in &cell.borrow().sym {
                if let Some(sym) = sym.upgrade() {
                    self.set_cell(sym, state)?;
                }
            }
            self.consistify10(cell)?;
            self.next_set += 1;
        }
        Ok(())
    }

    // 恢复到上一次设定自由的未知细胞的值之前，并切换细胞的状态
    fn backup(&mut self) -> Result<(), ()> {
        self.next_set = self.set_table.len();
        while self.next_set > 0 {
            self.next_set -= 1;
            let cell = self.set_table[self.next_set].upgrade().unwrap();
            self.set_table.pop();
            if cell.borrow().free {
                let state = match cell.borrow().state.unwrap() {
                    State::Dead => State::Alive,
                    State::Alive => State::Dead,
                };
                cell.borrow_mut().state = Some(state);
                cell.borrow_mut().free = false;
                self.set_table.push(Rc::downgrade(&cell));
                return Ok(());
            } else {
                cell.borrow_mut().state = None;
                cell.borrow_mut().free = true;
            }
        }
        Err(())
    }

    // 走；不对就退回来，换一下细胞的状态，再走，如此下去
    fn go(&mut self) -> Result<(), ()> {
        loop {
            if self.proceed().is_ok() {
                return Ok(());
            } else {
                self.backup()?;
            }
        }
    }

    // 最终搜索函数
    pub fn search(&mut self) -> Result<(), ()> {
        if self.time {
            self.stopwatch.restart();
        }
        if let None = self.world.get_unknown() {
            self.backup()?;
        }
        while self.go().is_ok() {
            if let Some(cell) = self.world.get_unknown() {
                cell.borrow_mut().state = Some(State::Dead);
                cell.borrow_mut().free = true;
                self.set_table.push(Rc::downgrade(&cell));
            } else if self.world.subperiod() {
                return Ok(());
            } else {
                self.backup()?;
            }
        }
        Err(())
    }

    pub fn display(&self) {
        self.world.display();
        if self.time {
            println!("Time taken: {}ms.", self.stopwatch.elapsed_ms());
        }
    }
}
