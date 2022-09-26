#![feature(variant_count)]
#![feature(exclusive_range_pattern)]

use pathfinding::prelude::{astar, dijkstra};
use std::collections::HashSet;
use std::fmt::{Debug, Display, Formatter, Write};
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum MoveLocation {
    BlockMinorPiles,
    PlayingArea { index: usize },
}

impl Display for MoveLocation {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            MoveLocation::BlockMinorPiles => f.write_str("BLOCK"),
            MoveLocation::PlayingArea { index } => Display::fmt(index, f),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Move {
    from: MoveLocation,
    to: MoveLocation,
    card: Card,
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

const NUM_SUITS: usize = std::mem::variant_count::<Suit>();

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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Board {
    major_lower_stack: Vec<Card>,
    major_higher_stack: Vec<Card>,
    minor_collection_piles: [Vec<Card>; NUM_SUITS],
    minor_collection_blocked: Option<Card>,
    playing_area: [Vec<Card>; NUM_PLAYING_STACKS],
}

impl Board {
    fn start(&mut self) {
        self.suck_readies_into_receptacles();
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

    fn score_lower_is_better(&self) -> usize {
        self.playing_area.iter().map(|stack| stack.len()).sum()
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
                    match last_card {
                        Card::Minor {
                            value: MinorValue(2),
                            ..
                        } => {
                            // println!(
                            //     "{:?} {:?}",
                            //     &self
                            //         .last_card_of_every_stack()
                            //         .map(|card| card.map(|card| card.to_string())),
                            //     self.minor_collection_blocked.is_some(),
                            // );
                            // panic!("seen a 2 here but didn't suck it, what da hell")
                        }
                        _ => (),
                    }

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
        }

        sucked_cards
    }

    fn last_card_of_every_stack(&self) -> [Option<Card>; NUM_PLAYING_STACKS] {
        let mut last_cards = [None; NUM_PLAYING_STACKS];
        for (playing_area_index, stack) in self.playing_area.iter().enumerate() {
            last_cards[playing_area_index] = stack.last().cloned();
        }
        last_cards
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

    fn next_boards(&self) -> Vec<(Self, Move)> {
        let mut boards = vec![];

        for (src_index, src_stack) in self.playing_area.iter().enumerate() {
            let src_card = src_stack.last().copied();
            if src_card.is_none() {
                continue;
            }
            let src_card = src_card.unwrap();

            if self.minor_collection_blocked.is_none() {
                let mut new_board = self.clone();
                let card = new_board.playing_area[src_index].pop().unwrap();
                new_board.minor_collection_blocked = Some(card);
                new_board.start();
                let moov = Move {
                    from: MoveLocation::PlayingArea { index: src_index },
                    to: MoveLocation::BlockMinorPiles,
                    card,
                };
                boards.push((new_board, moov));
            }

            for (dst_index, dst_stack) in self.playing_area.iter().enumerate() {
                if src_index == dst_index {
                    continue;
                }
                if dst_stack.is_empty() || dst_stack.last().unwrap().is_next_or_prev(src_card) {
                    let mut new_board = self.clone();
                    let src_card = new_board.playing_area[src_index].pop().unwrap();
                    new_board.playing_area[dst_index].push(src_card);
                    new_board.start();
                    let moov = Move {
                        from: MoveLocation::PlayingArea { index: src_index },
                        to: MoveLocation::PlayingArea { index: dst_index },
                        card: src_card,
                    };
                    boards.push((new_board, moov));
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
                    new_board.start();
                    let moov = Move {
                        from: MoveLocation::BlockMinorPiles,
                        to: MoveLocation::PlayingArea { index: dst_index },
                        card,
                    };
                    boards.push((new_board, moov));
                }
            }
        }

        boards
    }
}

fn main() {
    let init = r#"Q_SWO,9_CUP,2_STA,0_MAJ,2_CUP,8_STA,8_SWO
    J_CUP,2_WAN,10_SWO,10_CUP,Q_CUP,6_CUP,20_MAJ
    3_CUP,16_MAJ,7_WAN,J_WAN,14_MAJ,5_MAJ,4_WAN
    Q_STA,10_STA,J_SWO,8_CUP,3_STA,4_MAJ,8_MAJ
    13_MAJ,5_CUP,7_STA,10_MAJ,11_MAJ,Q_WAN,12_MAJ
    
    3_MAJ,2_MAJ,J_STA,7_CUP,4_CUP,7_MAJ,4_STA
    7_SWO,18_MAJ,9_WAN,3_SWO,K_WAN,2_SWO,9_MAJ
    6_MAJ,5_SWO,17_MAJ,9_SWO,5_WAN,6_STA,6_WAN
    K_SWO,6_SWO,8_WAN,4_SWO,21_MAJ,5_STA,3_WAN
    15_MAJ,1_MAJ,10_WAN,9_STA,K_STA,K_CUP,19_MAJ"#;
    let mut b = Board::parse(init);
    b.start();
    dbg!(&b);

    let (path, score): (Vec<(Board, Option<Move>)>, usize) = astar(
        &(b, None),
        |(b, path)| {
            b.next_boards()
                .iter()
                .map(|(board, moov)| ((board.clone(), Some(*moov)), 1))
                .collect::<Vec<_>>()
        },
        |(b, _move)| b.score_lower_is_better(),
        |(b, _move)| b.is_done(),
    )
    .unwrap();
    dbg!(path
        .iter()
        .filter_map(|i| i.1.map(|i| i.to_string()))
        .collect::<Vec<_>>());
}
