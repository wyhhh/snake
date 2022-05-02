use core::fmt;
use crossterm::cursor;
use std::fmt::Display;
use std::hint::unreachable_unchecked;
use std::intrinsics::transmute;
use std::io::stdout;
use std::marker::PhantomData;
use std::mem;
use std::mem::size_of;
use std::mem::MaybeUninit;
use std::ptr::null_mut;
use std::thread;
use std::time::Duration;
use std::time::Instant;
use vec_list::*;
use wutil::init_static_array;
use wutil::random::gen;

const SNAKES: usize = 4;
const DEALY_MILLIS: u64 = 50;

fn main() {
    crossterm::execute! {
        stdout(),
        cursor::Hide,
    };

    let mut game = Game::init();
    let mut tot_time = Duration::ZERO;
    let mut frames = 0.0;

    loop {
        let now = Instant::now();

        let max_tier = game.mov();
        thread::sleep(Duration::from_millis(DEALY_MILLIS));

        game.draw();

        let elapse = now.elapsed();
        tot_time += elapse;
        frames += 1.0;

        println!(
            "Turn: {:.0?}  FPS: {:.1}  Max-tier: {}  Bingos: {}",
            elapse,
            frames / tot_time.as_secs_f32(),
            max_tier,
            game.bingos
        );

        print!("\x1B[2J\x1B[1;1H");
    }
}

mod board {
    use crate::Tier;
    use crate::Tiers;
    use std::intrinsics::transmute;
    use std::mem::size_of;
    use vec_list::*;
    use wutil::init_static_array;
    use wutil::random::gen;

    pub const HEIGHT: i16 = 20;
    pub const WIDTH: i16 = 80;
    pub static mut BOARD: [[Tiers; WIDTH as usize]; HEIGHT as usize] = init_static_array!(
        Tiers { tiers: vec_list![] },
        size_of::<Tiers>(),
        HEIGHT as usize * WIDTH as usize
    );

    /// Safety: Assume in range.
    pub unsafe fn get((y, x): (i16, i16)) -> &'static mut Tiers {
        debug_assert!(y < HEIGHT);
        debug_assert!(x < WIDTH);
        BOARD
            .get_unchecked_mut(y as usize)
            .get_unchecked_mut(x as usize)
    }

    pub unsafe fn push((y, x): (i16, i16), tier: Tier) -> usize {
        get((y, x)).tiers.push_back(tier)
    }

    pub fn push_flower() -> (i16, i16, usize) {
        unsafe {
            let p = random_point();
            let tier = push(p, Tier::flower());
            (p.0, p.1, tier)
        }
    }

    pub unsafe fn get_tier((y, x, z): (i16, i16, usize)) -> &'static mut Tier {
        get((y, x)).tiers.get_unchecked_mut(z as usize)
    }

    pub fn random_point() -> (i16, i16) {
        let num = gen(0..HEIGHT * WIDTH);
        let (y, x) = (num / WIDTH, num % WIDTH);

        (y, x)
    }

    pub fn point_by_dir(
        (base_y, base_x): (i16, i16),
        (y_delta, x_delta): (i16, i16),
    ) -> (i16, i16) {
        (
            (base_y + y_delta + HEIGHT) % HEIGHT,
            (base_x + x_delta + WIDTH) % WIDTH,
        )
    }
}

const DIR: [(i16, i16); 8] = [
    (-1, -1),
    (-1, 0),
    (-1, 1),
    (0, -1),
    (0, 1),
    (1, -1),
    (1, 0),
    (1, 1),
];

pub struct Tiers {
    tiers: VecList<Tier>,
}
#[derive(Debug)]
pub enum NodeType {
    LaughHead,
    CryHead,
    Body,
}

#[derive(Debug)]
pub enum Tier {
    Node {
        node_type: NodeType,
        prev: Option<(i16, i16, usize)>,
    },
    Flower,
    Grass,
}

#[derive(Debug)]
pub enum TierType {
    Head,
    Body,
    Flower,
    Grass,
}

#[derive(Debug, Clone, Copy, Default)]
struct Snake {
    head: (i16, i16, usize),
    tail: (i16, i16, usize),
}

struct Game {
    snakes: [Snake; SNAKES],
    flower: (i16, i16, usize),
    lucky_guy: Option<usize>,
    bingos: u32,
}

impl Tier {
    fn laugh_head() -> Self {
        Self::Node {
            node_type: NodeType::LaughHead,
            prev: None,
        }
    }

    fn cry_head() -> Self {
        Self::Node {
            node_type: NodeType::CryHead,
            prev: None,
        }
    }

    fn body(prev: (i16, i16, usize)) -> Self {
        Self::Node {
            node_type: NodeType::Body,
            prev: Some(prev),
        }
    }

    fn grass() -> Self {
        Self::Grass
    }

    fn flower() -> Self {
        Self::Flower
    }

    fn is_grass(&self) -> bool {
        matches!(self, Self::Grass)
    }

    fn is_body(&self) -> bool {
        match self {
            Tier::Node { node_type, .. } => matches!(node_type, NodeType::Body),
            _ => false,
        }
    }

    fn is_laugh_head(&self) -> bool {
        match self {
            Tier::Node { node_type, .. } => matches!(node_type, NodeType::LaughHead),
            _ => false,
        }
    }
}

impl Snake {
    const fn zerod() -> Self {
        Self {
            head: (0, 0, 0),
            tail: (0, 0, 0),
        }
    }
    fn random() -> Self {
        let head = board::random_point();
        let tail_dir = *unsafe { DIR.get_unchecked(gen(0..8) as usize) };
        let tail = board::point_by_dir(head, tail_dir);

        let head_tier = unsafe { board::push(head, Tier::cry_head()) };
        let tail_tier = unsafe { board::push(tail, Tier::body((head.0, head.1, head_tier))) };

        Self {
            head: (head.0, head.1, head_tier),
            tail: (tail.0, tail.1, tail_tier),
        }
    }
}

impl Game {
    fn init() -> Self {
        /* init snakes */
        let mut snakes: [Snake; SNAKES] =
            init_static_array!(Snake::zerod(), size_of::<Snake>(), SNAKES);

        for snake in &mut snakes {
            *snake = Snake::random();
        }

        /* init flower */
        let flower = board::push_flower();

        Self {
            snakes,
            flower,
            lucky_guy: None,
            bingos: 0,
        }
    }

    fn min_flower_distance_point(flower: (i16, i16), head: (i16, i16)) -> (i16, i16) {
        let mut min = i64::MAX;
        let mut min_yx = (0, 0);

        for delta in DIR {
            let point = board::point_by_dir(head, delta);
            let distance = Self::fake_flower_distance(flower, point);

            if distance < min {
                min = distance;
                min_yx = point;
            }
        }

        min_yx
    }

    fn fake_flower_distance(flower: (i16, i16), (y, x): (i16, i16)) -> i64 {
        let y_dif = (flower.0 - y) as i64;
        let x_dif = (flower.1 - x) as i64;
        // let Y_dif = board::HEIGHT as i64 - y_dif.abs();
        // let X_dif = board::WIDTH as i64 - x_dif.abs();
        // ((y_dif * y_dif) + (x_dif * x_dif)).min((Y_dif * Y_dif) + (X_dif * X_dif))
        (y_dif * y_dif) + (x_dif * x_dif)
    }

    fn mov(&mut self) -> usize {
        let mut max_tier = 0;
        for (idx, snake) in self.snakes.iter_mut().enumerate() {
            let old_head = snake.head;
            let old_tail = snake.tail;
            /* HEAD */
            // 1. get new head yx
            let new_head = Self::min_flower_distance_point(
                (self.flower.0, self.flower.1),
                (old_head.0, old_head.1),
            );
            let bingo = (self.flower.0, self.flower.1) == (new_head.0, new_head.1);
            let new_head_tiers = unsafe { board::get(new_head) };
            max_tier = max_tier.max(new_head_tiers.tiers.len());

            let new_head_tier = new_head_tiers
                .tiers
                .push_back(if let Some(l) = self.lucky_guy {
                    if l == idx {
                        Tier::laugh_head()
                    } else {
                        Tier::cry_head()
                    }
                } else {
                    Tier::cry_head()
                });

            if bingo {
                let flower_tiers = unsafe { board::get((self.flower.0, self.flower.1)) };
                max_tier = max_tier.max(flower_tiers.tiers.len());
                let mut flower_tier =
                    unsafe { flower_tiers.tiers.get_unchecked_mut(self.flower.2) as *mut Tier };

                if let Some(prev_idx) = flower_tiers.tiers.previous(self.flower.2) {
                    if unsafe { flower_tiers.tiers.get_unchecked(prev_idx) }.is_grass() {
                        flower_tiers.tiers.delete(self.flower.2);
                    } else {
                        unsafe {
                            *flower_tier = Tier::grass();
                        }
                    }
                } else {
                    unsafe {
                        *flower_tier = Tier::grass();
                    }
                };

                self.flower = board::push_flower();
                self.lucky_guy = Some(idx);
                self.bingos += 1;
            }

            let old_head = unsafe { board::get_tier(old_head) };

            *old_head = Tier::body((new_head.0, new_head.1, new_head_tier));

            snake.head = (new_head.0, new_head.1, new_head_tier);

            if bingo {
                return max_tier;
            }

            /* TAIL */
            let old_tail_tiers = unsafe { board::get((old_tail.0, old_tail.1)) };
            max_tier = max_tier.max(old_tail_tiers.tiers.len());
            let old_tail_tier = unsafe { old_tail_tiers.tiers.get_unchecked_mut(old_tail.2) };

            debug_assert!(old_tail_tier.is_body());

            match old_tail_tier {
                Tier::Node { prev, .. } => {
                    debug_assert!(prev.is_some());
                    snake.tail = unsafe { prev.unwrap_unchecked() }
                }
                _ => unsafe { unreachable_unchecked() },
            }

            old_tail_tiers.tiers.delete(old_tail.2);
        }

        max_tier
    }

    fn draw(&mut self) {
        for row in unsafe { board::BOARD.iter() } {
            for grid in row {
                print!("{:?}", grid);
            }
            println!();
        }
        println!();
    }
}

impl fmt::Debug for Tiers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let chr = if let Some((tier, _)) = self.tiers.back() {
            match tier {
                Tier::Node { node_type, .. } => match node_type {
                    NodeType::LaughHead => '😁',
                    NodeType::CryHead => '😭',
                    NodeType::Body => '🌸',
                },
                Tier::Flower => '🌹',
                Tier::Grass => '🍀',
            }
        } else {
            ' '
        };

        write!(f, "{}", chr)
    }
}
