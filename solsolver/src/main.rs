#![feature(variant_count)]
#![feature(const_option)]
#![feature(const_for)]

use cap::Cap;
use derivative::Derivative;
use pathfinding::prelude::astar;
use rayon::prelude::*;
use std::alloc;
use std::collections::VecDeque;
use std::fmt::{Debug, Display, Formatter};
use std::hash::Hash;
use std::io::{stdin, Read};

// TODO: can we query how much memory's on the machine?
const MEMORY_LIMIT_BYTES: usize = 8 * 1024 * 1024 * 1024;

// HAX: sometimes we're not actually able to solve the position, i don't know why. but if we limit
// the memory usage of the global allocator, we can more gracefully exit without taking down the
// rest of the system
//
// there's something over on the python side that'll just restart the whole game and try to solve
// a new position if we exit with a non-zero exit code, which is what happens when this Cap limited
// global allocator runs out of memory
#[global_allocator]
static ALLOCATOR: Cap<alloc::System> = Cap::new(alloc::System, MEMORY_LIMIT_BYTES);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum MoveLocation {
    BlockMinorPiles,
    PlayingArea { pile: usize, depth: usize },
}

impl Display for MoveLocation {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            MoveLocation::BlockMinorPiles => f.write_str("BLOCK"),
            MoveLocation::PlayingArea { pile, depth: _ } => Display::fmt(pile, f),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Move {
    from: MoveLocation,
    to: MoveLocation,
    card: Card,
    // we count the number of sucks, so that in the GUI automation side, we know how long
    // to wait before the next move
    num_sucks: usize,
}

impl Display for Move {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("Card ")?;
        Display::fmt(&self.card, f)?;
        f.write_str(" Pile ")?;
        Display::fmt(&self.from, f)?;
        f.write_str(" -> Pile ")?;
        Display::fmt(&self.to, f)
    }
}

impl Move {
    fn serialize(&self) -> String {
        format!(
            "{}-{}@{}@{}",
            self.from.serialize(),
            self.to.serialize(),
            self.num_sucks,
            self,
        )
    }
}

impl MoveLocation {
    fn serialize(&self) -> String {
        match self {
            MoveLocation::BlockMinorPiles => "BLOCK".to_string(),
            MoveLocation::PlayingArea { pile, depth } => format!("{}:{}", pile, depth),
        }
    }
}

// Ace = 1
// 2 = 2
// 3 = 3
// ...
// 10 = 10
// J = 11
// Q = 12
// K = 13
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
struct MinorValue(u8);

impl Debug for MinorValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            1 => f.write_str("A"),
            11 => f.write_str("J"),
            12 => f.write_str("Q"),
            13 => f.write_str("K"),
            otherwise => Debug::fmt(&otherwise, f),
        }
    }
}

impl Display for MinorValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Debug::fmt(self, f)
    }
}

impl MinorValue {
    fn parse(s: &str) -> Self {
        match s {
            "A" => MinorValue(1),
            "2" => MinorValue(2),
            "3" => MinorValue(3),
            "4" => MinorValue(4),
            "5" => MinorValue(5),
            "6" => MinorValue(6),
            "7" => MinorValue(7),
            "8" => MinorValue(8),
            "9" => MinorValue(9),
            "10" => MinorValue(10),
            "J" => MinorValue(11),
            "Q" => MinorValue(12),
            "K" => MinorValue(13),
            otherwise => panic!("Invalid minor value: {}", otherwise),
        }
    }
}

// from 0 to 21
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
struct MajorValue(u8);

impl MajorValue {
    fn parse(s: &str) -> Self {
        MajorValue(s.parse().unwrap())
    }

    const fn first() -> Self {
        MajorValue(0)
    }

    const fn last() -> Self {
        MajorValue(21)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(usize)]
enum Suit {
    Sword,
    Wand,
    Cup,
    Star,
}

const NUM_SUITS: usize = std::mem::variant_count::<Suit>();

impl Display for Suit {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Suit::Sword => f.write_str("🗡"),
            Suit::Wand => f.write_str("🪄"),
            Suit::Cup => f.write_str("🍷"),
            Suit::Star => f.write_str("⭐"),
        }
    }
}

impl Suit {
    fn parse(s: &str) -> Self {
        match s {
            "SWO" => Suit::Sword,
            "WAN" => Suit::Wand,
            "CUP" => Suit::Cup,
            "STA" => Suit::Star,
            otherwise => panic!("Invalid suit: {}", otherwise),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Card {
    Major(MajorValue),
    Minor { suit: Suit, value: MinorValue },
}

impl Display for Card {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Card::Major(value) => Display::fmt(&value.0, f),
            Card::Minor { suit, value } => {
                Display::fmt(value, f)?;
                Display::fmt(suit, f)
            }
        }
    }
}

impl Card {
    fn parse(s: &str) -> Self {
        let mut split = s.split('_');
        let value = split.next().unwrap();
        let suit = split.next().unwrap();
        if suit == "MAJ" {
            Card::Major(MajorValue::parse(value))
        } else {
            Card::Minor {
                suit: Suit::parse(suit),
                value: MinorValue::parse(value),
            }
        }
    }

    fn is_next_card(self, next_card: Self) -> bool {
        match (self, next_card) {
            (Card::Major(this_value), Card::Major(next_value)) => this_value.0 + 1 == next_value.0,
            (
                Card::Minor {
                    suit,
                    value: this_value,
                },
                Card::Minor {
                    suit: next_suit,
                    value: next_value,
                },
            ) => suit == next_suit && this_value.0 + 1 == next_value.0,
            _ => false,
        }
    }

    fn is_prev_card(self, prev_card: Self) -> bool {
        match (self, prev_card) {
            (Card::Major(MajorValue(this_val)), Card::Major(MajorValue(prev_val))) => {
                this_val == prev_val + 1
            }
            (
                Card::Minor {
                    suit,
                    value: this_val,
                },
                Card::Minor {
                    suit: prev_suit,
                    value: prev_val,
                },
            ) => suit == prev_suit && this_val.0 == prev_val.0 + 1,
            _ => false,
        }
    }

    fn is_next_or_prev(self, other: Self) -> bool {
        self.is_next_card(other) || self.is_prev_card(other)
    }
}

const NUM_PLAYING_STACKS: usize = 11;

// #[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[derive(Debug, Clone, Derivative)]
#[derivative(PartialEq, Eq, Hash)]
struct Board {
    major_lower_stack: Vec<Card>,
    major_higher_stack: Vec<Card>,
    minor_collection_piles: [Vec<Card>; NUM_SUITS],
    minor_collection_blocked: Option<Card>,
    playing_area: [Vec<Card>; NUM_PLAYING_STACKS],
    #[derivative(PartialEq = "ignore", Hash = "ignore")]
    last_n_moves: VecDeque<Move>,
}

// HAX: OLD is used to indicate that we don't limit the num_prev_moves, and instead use what we were using before: no limit to prune the search tree. SOMETIMES that was producing better results
const OLD: usize = 0;
const NUM_PREV_MOVES_TO_CONSIDERS: [usize; 4] = [5, 10, 15, OLD];

const fn const_max(ns: &[usize]) -> usize {
    let mut max = 0;
    let mut i = 0;
    while i < ns.len() {
        if ns[i] > max {
            max = ns[i];
        }
        i += 1;
    }
    max
}

const MAX_NUM_PREV_MOVES_TO_CONSIDER: usize = const_max(&NUM_PREV_MOVES_TO_CONSIDERS);

impl Board {
    fn with_prev_move(self, prev_move: Move) -> Self {
        let mut new_board = self;
        new_board.last_n_moves.push_front(prev_move);
        if new_board.last_n_moves.len() > MAX_NUM_PREV_MOVES_TO_CONSIDER {
            new_board.last_n_moves.pop_back();
        }
        new_board
    }

    fn is_done(&self) -> bool {
        self.playing_area.iter().all(|pile| pile.is_empty())
    }

    fn parse(s: &str) -> Self {
        let mut playing_area = [
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        ];
        for (line, stack_to_fill) in s.lines().zip(playing_area.iter_mut()) {
            for card in line.trim().split_terminator(',') {
                let card = Card::parse(card);
                stack_to_fill.push(card);
            }
        }
        Self {
            last_n_moves: VecDeque::new(),
            major_higher_stack: vec![],
            major_lower_stack: vec![],
            minor_collection_piles: [
                vec![Card::Minor {
                    suit: Suit::Sword,
                    value: MinorValue(1),
                }],
                vec![Card::Minor {
                    suit: Suit::Wand,
                    value: MinorValue(1),
                }],
                vec![Card::Minor {
                    suit: Suit::Cup,
                    value: MinorValue(1),
                }],
                vec![Card::Minor {
                    suit: Suit::Star,
                    value: MinorValue(1),
                }],
            ],
            minor_collection_blocked: None,
            playing_area,
        }
    }

    fn num_cards_remaining(&self) -> usize {
        self.playing_area
            .iter()
            .map(|stack| stack.len())
            .sum::<usize>()
        // TODO: sometimes commenting this bottom part out makes it so we complete instead of not completing ???
        + self.minor_collection_blocked.is_some() as usize
    }

    fn suck_readies_into_receptacles(&mut self) -> Vec<Card> {
        let mut sucked_cards = vec![];

        let mut changed = true;
        while changed {
            changed = false;

            for (playing_area_index, last_card) in
                self.last_card_of_every_stack_mut().into_iter().enumerate()
            {
                if last_card.is_none() {
                    continue;
                }
                let last_card = last_card.unwrap();

                // see if we can suck into minor collection pile
                if self.minor_collection_blocked.is_none() {
                    for minor_collection_pile in self.minor_collection_piles.iter_mut() {
                        if minor_collection_pile
                            .last()
                            .unwrap()
                            .is_next_card(last_card)
                        {
                            let card = self.playing_area[playing_area_index].pop().unwrap();
                            minor_collection_pile.push(card);
                            sucked_cards.push(card);
                            changed = true;
                        }
                    }
                }

                // see if we can suck into one of the major collection piles
                if self
                    .major_lower_stack
                    .last()
                    .map(|card| card.is_next_card(last_card))
                    .unwrap_or(false)
                    || (self.major_lower_stack.is_empty()
                        && last_card == Card::Major(MajorValue::first()))
                {
                    let card = self.playing_area[playing_area_index].pop().unwrap();
                    self.major_lower_stack.push(card);
                    sucked_cards.push(card);
                    changed = true;
                } else if self
                    .major_higher_stack
                    .last()
                    .map(|card| card.is_prev_card(last_card))
                    .unwrap_or(false)
                    || (self.major_higher_stack.is_empty()
                        && last_card == Card::Major(MajorValue::last()))
                {
                    let card = self.playing_area[playing_area_index].pop().unwrap();
                    self.major_higher_stack.push(card);
                    sucked_cards.push(card);
                    changed = true;
                }
            }

            if let Some(blocking_card) = self.minor_collection_blocked {
                // TODO: slight duplication with above logic
                if self
                    .major_lower_stack
                    .last()
                    .map(|card| card.is_next_card(blocking_card))
                    .unwrap_or(false)
                {
                    self.major_lower_stack.push(blocking_card);
                    self.minor_collection_blocked = None;
                    sucked_cards.push(blocking_card);
                    changed = true;
                } else if self
                    .major_higher_stack
                    .last()
                    .map(|card| card.is_prev_card(blocking_card))
                    .unwrap_or(false)
                {
                    self.major_higher_stack.push(blocking_card);
                    self.minor_collection_blocked = None;
                    sucked_cards.push(blocking_card);
                    changed = true;
                }
            }
        }

        sucked_cards
    }

    fn last_card_of_every_stack_mut(&mut self) -> [Option<Card>; 11] {
        let mut last_cards = [None; NUM_PLAYING_STACKS];
        for (stack, last_card) in self.playing_area.iter().zip(last_cards.iter_mut()) {
            if let Some(card) = stack.last().copied() {
                *last_card = Some(card);
            }
        }
        last_cards
    }

    fn next_boards(&self, num_prev_moves_to_consider: usize) -> Vec<(Self, Move)> {
        let mut boards = vec![];

        const MINIMUM_AMT_OF_PROGRESS: usize = 1;

        if num_prev_moves_to_consider != OLD
            && self.last_n_moves.len() >= num_prev_moves_to_consider
            && self
                .last_n_moves
                .iter()
                .take(num_prev_moves_to_consider)
                .map(|m| m.num_sucks)
                .sum::<usize>()
                <= MINIMUM_AMT_OF_PROGRESS
        {
            return boards;
        }

        for (src_index, src_stack) in self.playing_area.iter().enumerate() {
            let src_card = src_stack.last().copied();
            if src_card.is_none() {
                continue;
            }
            let src_card = src_card.unwrap();

            if self.minor_collection_blocked.is_none() {
                // // filters out a useless move: there is never any reason to block the minor pile
                // // from a stack that only has one card
                if src_stack.len() == 1 {
                    continue;
                }

                let mut new_board = self.clone();
                let card = new_board.playing_area[src_index].pop().unwrap();
                new_board.minor_collection_blocked = Some(card);
                let sucked_cards = new_board.suck_readies_into_receptacles();
                let moov = Move {
                    from: MoveLocation::PlayingArea {
                        pile: src_index,
                        depth: self.playing_area[src_index].len() - 1,
                    },
                    to: MoveLocation::BlockMinorPiles,
                    card,
                    num_sucks: sucked_cards.len(),
                };
                boards.push((new_board.with_prev_move(moov), moov));
            }

            for (dst_index, dst_stack) in self.playing_area.iter().enumerate() {
                // moving a card to its own stack isn't a move
                if src_index == dst_index {
                    continue;
                }
                // filters out a non-progress move: moving a card from a 1-stack to another 1-stack
                if src_stack.len() == 1 && dst_stack.is_empty() {
                    continue;
                }
                if dst_stack.is_empty() || dst_stack.last().unwrap().is_next_or_prev(src_card) {
                    let mut new_board = self.clone();
                    let src_card = new_board.playing_area[src_index].pop().unwrap();
                    new_board.playing_area[dst_index].push(src_card);
                    let sucked_cards = new_board.suck_readies_into_receptacles();
                    let moov = Move {
                        from: MoveLocation::PlayingArea {
                            pile: src_index,
                            depth: self.playing_area[src_index].len() - 1,
                        },
                        to: MoveLocation::PlayingArea {
                            pile: dst_index,
                            depth: self.playing_area[dst_index].len(),
                        },
                        card: src_card,
                        num_sucks: sucked_cards.len(),
                    };
                    boards.push((new_board.with_prev_move(moov), moov));
                }
            }
        }

        // unblock the minor collection piles
        if let Some(card) = self.minor_collection_blocked {
            // TODO: this is duplicated from above, we could consolidate if need be
            for (dst_index, dst_stack) in self.playing_area.iter().enumerate() {
                if dst_stack.is_empty() || dst_stack.last().unwrap().is_next_or_prev(card) {
                    let mut new_board = self.clone();
                    let card = new_board.minor_collection_blocked.take().unwrap();
                    new_board.playing_area[dst_index].push(card);
                    let sucked_cards = new_board.suck_readies_into_receptacles();
                    let moov = Move {
                        from: MoveLocation::BlockMinorPiles,
                        to: MoveLocation::PlayingArea {
                            pile: dst_index,
                            depth: self.playing_area[dst_index].len(),
                        },
                        card,
                        num_sucks: sucked_cards.len(),
                    };
                    boards.push((new_board.with_prev_move(moov), moov));
                }
            }
        }

        boards
    }
}

fn main() {
    let mut init = String::new();
    stdin().read_to_string(&mut init).unwrap();
    let mut b = Board::parse(&init);
    b.suck_readies_into_receptacles();
    dbg!(&b);

    let path = NUM_PREV_MOVES_TO_CONSIDERS
        .into_par_iter()
        .filter_map(|num_prev_moves| {
            let (path, _score): (Vec<(Board, Option<Move>)>, usize) = astar(
                &(b.clone(), None),
                |(b, _path)| {
                    b.next_boards(num_prev_moves)
                        .into_iter()
                        .map(|(board, moov)| ((board.clone(), Some(moov)), 0))
                },
                |(b, _move)| b.num_cards_remaining(),
                |(b, _move)| b.is_done(),
            )?;
            Some(path)
        })
        .min_by_key(|path| path.len())
        .unwrap();

    for moov in path.iter().filter_map(|(_, moov)| moov.as_ref()) {
        eprintln!("{} ({} sucks)", moov, moov.num_sucks);
        println!("{}", moov.serialize());
    }
}
