use std::rc::{Rc, Weak};
use std::str::FromStr;
use crate::cell::{Desc, State, LifeCell, RcCell, WeakCell};
use crate::cell::State::{Dead, Alive};

// 把规则写成一个 Trait，方便以后支持更多的规则
pub trait Rule {
    type Desc: Desc;

    // 规则是否是 B0
    fn b0(&self) -> bool;

    // 由一个细胞及其邻域的状态得到其后一代的状态
    fn transition(&self, state: Option<State>, desc: Self::Desc) -> Option<State>;

    // 由一个细胞的邻域以及其后一代的状态，决定其本身的状态
    fn implication(&self, desc: Self::Desc, succ_state: State) -> Option<State>;

    // 由一个细胞本身、邻域以及其后一代的状态，改变其邻域中某些未知细胞的状态
    // 并把改变了值的细胞放到 set_table 中
    fn consistify_nbhd(&self, cell: &RcCell<Self::Desc>, desc: Self::Desc, state: Option<State>,
        succ_state: State, set_table: &mut Vec<WeakCell<Self::Desc>>);
}

// 对称性
pub enum Symmetry {
    C1,
    C2,
    C4,
    D2Row,
    D2Column,
    D2Diag,
    D2Antidiag,
    D4Ortho,
    D4Diag,
    D8,
}

impl FromStr for Symmetry {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "C1" => Ok(Symmetry::C1),
            "C2" => Ok(Symmetry::C2),
            "C4" => Ok(Symmetry::C4),
            "D2|" => Ok(Symmetry::D2Row),
            "D2-" => Ok(Symmetry::D2Column),
            "D2\\" => Ok(Symmetry::D2Diag),
            "D2/" => Ok(Symmetry::D2Antidiag),
            "D4+" => Ok(Symmetry::D4Ortho),
            "D4X" => Ok(Symmetry::D4Diag),
            "D8" => Ok(Symmetry::D8),
            _ => Err(String::from("invalid symmetry")),
        }
    }
}

// 横座标，纵座标，时间
type Coord = (isize, isize, isize);

// 世界，暂时只适用于 Life-Like 的规则
pub struct World<D: Desc, R: Rule<Desc = D>> {
    pub width: isize,
    pub height: isize,
    pub period: isize,
    pub rule: R,

    // 搜索顺序是先行后列还是先列后行
    // 通过比较行数和列数的大小来自动决定
    column_first: bool,

    // 搜索范围内的所有细胞的列表
    cells: Vec<RcCell<D>>,

    // 公用的搜索范围外的死细胞
    dead_cell: RcCell<D>,
}

impl<D: Desc, R: Rule<Desc = D>> World<D, R> {
    pub fn new(width: isize, height: isize, period: isize, dx: isize, dy: isize,
        symmetry: Symmetry, rule: R, column_first: Option<bool>) -> Self {
        // 自动决定搜索顺序
        let column_first = match column_first {
            Some(c) => c,
            None => {
                let (width, height) = match symmetry {
                    Symmetry::D2Row => ((width + 1) / 2, height),
                    Symmetry::D2Column => (width, (height + 1) / 2),
                    _ => (width, height),
                };
                if width == height {
                    dx.abs() >= dy.abs()
                } else {
                    width > height
                }
            },
        };

        let b0 = rule.b0();

        let mut cells = Vec::with_capacity(((width + 2) * (height + 2) * period) as usize);

        // 先全部填上死细胞；如果是 B0 的规则，则在奇数代填上活细胞
        for _ in 0..(width + 2) * (height + 2) {
            for t in 0..period {
                let state = if b0 && t % 2 == 1 {
                    Alive
                } else {
                    Dead
                };
                cells.push(Rc::new(LifeCell::new(Some(state), false)));
            }
        }

        let dead_cell = Rc::new(LifeCell::new(Some(Dead), false));

        let life = World {width, height, period, rule, column_first, cells, dead_cell};

        // 先设定细胞的邻域
        // 注意：对于范围边缘的细胞，邻域可能指向不存在的细胞！
        let neighbors = [(-1,-1), (-1,0), (-1,1), (0,-1), (0,1), (1,-1), (1,0), (1,1)];
        for x in -1..width + 1 {
            for y in -1..height + 1 {
                for t in 0..period {
                    let cell = life.find_cell((x, y, t)).upgrade().unwrap();
                    for (i, (nx, ny)) in neighbors.iter().enumerate() {
                        cell.nbhd.borrow_mut()[i] = life.find_cell((x + nx, y + ny, t));
                    }
                }
            }
        }

        // 再给范围内的细胞添加别的信息
        for x in -1..width + 1 {
            for y in -1..height + 1 {
                for t in 0..period {
                    let cell = life.find_cell((x, y, t)).upgrade().unwrap();

                    // 默认的细胞状态
                    let default = if b0 && t % 2 == 1 {
                        Alive
                    } else {
                        Dead
                    };

                    // 用 set 设置细胞状态
                    if 0 <= x && x < width && 0 <= y && y < height {
                        cell.set(None, true);
                    }

                    // 设定前一代；若前一代不在范围内则把此细胞设为 default
                    if t != 0 {
                        *cell.pred.borrow_mut() = life.find_cell((x, y, t - 1));
                    } else {
                        let pred_coord = (x - dx, y - dy, period - 1);
                        let pred_weak = life.find_cell(pred_coord);
                        if pred_weak.upgrade().is_some() {
                            *cell.pred.borrow_mut() = pred_weak;
                        } else if 0 <= x && x < width && 0 <= y && y < height {
                            cell.set(Some(default), false);
                        }
                    }

                    // 设定后一代；若后一代不在范围内则把后一代设为 dead_cell
                    if t != period - 1 {
                        *cell.succ.borrow_mut() = life.find_cell((x, y, t + 1));
                    } else {
                        let succ_coord = (x + dx, y + dy, 0);
                        let succ_weak = life.find_cell(succ_coord);
                        if succ_weak.upgrade().is_some() {
                            *cell.succ.borrow_mut() = succ_weak;
                        } else {
                            *cell.succ.borrow_mut() = Rc::downgrade(&life.dead_cell);
                        }
                    }

                    // 设定对称的细胞；若对称的细胞不在范围内则把此细胞设为 default
                    let sym_coords = match symmetry {
                        Symmetry::C1 => vec![],
                        Symmetry::C2 => vec![(width - 1 - x, height - 1 - y, t)],
                        Symmetry::C4 => vec![(y, width - 1 - x, t),
                            (width - 1 - x, height - 1 - y, t),
                            (height - 1 - y, x, t)],
                        Symmetry::D2Row => vec![(width - 1 - x, y, t)],
                        Symmetry::D2Column => vec![(x, height - 1 - y, t)],
                        Symmetry::D2Diag => vec![(y, x, t)],
                        Symmetry::D2Antidiag => vec![(height - 1 - y, width - 1 - x, t)],
                        Symmetry::D4Ortho => vec![(width - 1 - x, y, t),
                            (x, height - 1 - y, t),
                            (width - 1 - x, height - 1 - y, t)],
                        Symmetry::D4Diag => vec![(y, x, t),
                            (height - 1 - y, width - 1 - x, t),
                            (width - 1 - x, height - 1 - y, t)],
                        Symmetry::D8 => vec![(y, width - 1 - x, t),
                            (height - 1 - y, x, t),
                            (width - 1 - x, y, t),
                            (x, height - 1 - y, t),
                            (y, x, t),
                            (height - 1 - y, width - 1 - x, t),
                            (width - 1 - x, height - 1 - y, t)],
                    };
                    for coord in sym_coords {
                        let sym_weak = life.find_cell(coord);
                        if 0 <= coord.0 && coord.0 < width &&
                            0 <= coord.1 && coord.1 < height {
                            cell.sym.borrow_mut().push(sym_weak);
                        } else if 0 <= x && x < width && 0 <= y && y < height {
                            cell.set(Some(default), false);
                        }
                    }
                }
            }
        }

        life
    }

    // 通过坐标查找细胞
    pub fn find_cell(&self, coord: Coord) -> WeakCell<D> {
        let (x, y, t) = coord;
        if x >= -1 && x <= self.width && y >= -1 && y <= self.height {
            let index = if self.column_first {
                ((x + 1) * (self.height + 2) + y + 1) * self.period + t
            } else {
                ((y + 1) * (self.width + 2) + x + 1) * self.period + t
            };
            Rc::downgrade(&self.cells[index as usize])
        } else {
            Weak::new()
        }
    }

    fn get_cell(&self, coord: Coord) -> (Option<State>, bool) {
        let cell = self.find_cell(coord).upgrade().unwrap();
        (cell.state.get(), cell.free.get())
    }

    // 显示某一代的整个世界
    pub fn display_gen(&self, t: isize) -> String {
        let mut str = String::new();
        let t = t % self.period;
        for y in 0..self.height {
            for x in 0..self.width {
                let s = match self.get_cell((x, y, t)).0 {
                    Some(Dead) => '.',
                    Some(Alive) => 'O',
                    None => '?',
                };
                str.push(s);
            }
            str.push('\n');
        }
        str
    }

    // 获取一个未知的细胞
    pub fn get_unknown(&self) -> WeakCell<D> {
        self.cells.iter().find(|cell| cell.state.get().is_none())
            .map(Rc::downgrade).unwrap_or_default()
    }

    // 确保搜出来的图样非空，而且最小周期不小于指定的周期
    pub fn nontrivial(&self) -> bool {
        let nonzero = self.cells.iter().step_by(self.period as usize)
            .any(|c| c.state.get() != Some(Dead));
        nonzero && (self.period == 1 ||
            (1..self.period).all(|t|
                self.period % t != 0 ||
                    self.cells.chunks(self.period as usize)
                        .any(|c| c[0].state.get() != c[t as usize].state.get())))
    }
}
