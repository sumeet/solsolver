#![feature(variant_count)]
#![feature(exclusive_range_pattern)]

use pathfinding::prelude::dijkstra;

// Ace = 1
// 2 = 2
// 3 = 3
// ...
// 10 = 10
// J = 11
// Q = 12
// K = 13
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
struct MinorValue(u8);

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

    fn is_next_card(self, other: Self) -> bool {
        match (self, other) {
            (Card::Major(major), Card::Major(other_major)) => major.0 == other_major.0 + 1,
            (
                Card::Minor { suit, value },
                Card::Minor {
                    suit: other_suit,
                    value: other_value,
                },
            ) => suit == other_suit && value.0 == other_value.0 + 1,
            _ => false,
        }
    }

    fn is_prev_card(self, other: Self) -> bool {
        match (self, other) {
            (Card::Major(MajorValue(major)), Card::Major(MajorValue(other_major @ (1..255)))) => {
                major == other_major - 1
            }
            (
                Card::Minor { suit, value },
                Card::Minor {
                    suit: other_suit,
                    value: other_value,
                },
            ) => suit == other_suit && value.0 == other_value.0 - 1,
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

    fn suck_readies_into_receptacles(&mut self) {
        let mut changed = true;
        while changed {
            changed = false;

            for (playing_area_index, last_card) in
                self.last_card_of_every_stack().into_iter().enumerate()
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
                    self.major_lower_stack
                        .push(self.playing_area[playing_area_index].pop().unwrap());
                    changed = true;
                } else if self
                    .major_higher_stack
                    .last()
                    .map(|card| card.is_prev_card(last_card))
                    .unwrap_or(false)
                    || (self.major_higher_stack.is_empty()
                        && last_card == Card::Major(MajorValue::last()))
                {
                    self.major_higher_stack
                        .push(self.playing_area[playing_area_index].pop().unwrap());
                    changed = true;
                }
            }
        }
    }

    fn last_card_of_every_stack(&mut self) -> [Option<Card>; 11] {
        let mut last_cards = [None; NUM_PLAYING_STACKS];
        for (stack, last_card) in self.playing_area.iter().zip(last_cards.iter_mut()) {
            if let Some(card) = stack.last().copied() {
                *last_card = Some(card);
            }
        }
        last_cards
    }

    fn next_boards(&self) -> Vec<Self> {
        let mut boards = vec![];

        for (src_index, src_stack) in self.playing_area.iter().enumerate() {
            let src_card = src_stack.last().copied();
            if src_card.is_none() {
                continue;
            }
            let src_card = src_card.unwrap();

            for (dst_index, dst_stack) in self.playing_area.iter().enumerate() {
                if src_index == dst_index {
                    continue;
                }
                if dst_stack.is_empty() || dst_stack.last().unwrap().is_next_or_prev(src_card) {
                    let mut new_board = self.clone();
                    let src_card = new_board.playing_area[src_index].pop().unwrap();
                    new_board.playing_area[dst_index].push(src_card);
                    new_board.start();
                    boards.push(new_board);
                }
            }
        }
        boards
    }
}

fn main() {
    let init = r#"7_MAJ,9_SWO,8_MAJ,0_MAJ,6_STA,18_MAJ,19_MAJ
J_CUP,8_WAN,14_MAJ,9_CUP,K_STA,10_CUP,10_SWO
K_WAN,J_SWO,3_WAN,8_CUP,J_STA,3_CUP,4_WAN
Q_CUP,3_SWO,21_MAJ,K_SWO,5_MAJ,7_WAN,9_MAJ
2_STA,Q_SWO,13_MAJ,2_SWO,5_CUP,4_CUP,5_WAN

16_MAJ,3_STA,20_MAJ,J_WAN,9_STA,5_STA,8_SWO
4_MAJ,2_WAN,3_MAJ,9_WAN,K_CUP,2_CUP,6_WAN
6_SWO,7_SWO,1_MAJ,Q_WAN,11_MAJ,7_STA,7_CUP
4_SWO,6_MAJ,Q_STA,6_CUP,10_MAJ,10_WAN,8_STA
2_MAJ,10_STA,5_SWO,15_MAJ,12_MAJ,4_STA,17_MAJ
"#;
    let mut b = Board::parse(init);
    b.start();
    dbg!(&b);
    let r = dijkstra(
        &b,
        |b| {
            b.next_boards().into_iter().map(|b| {
                let score = b.score_lower_is_better();
                dbg!(score);
                (b, score)
            })
        },
        Board::is_done,
    );
    dbg!(r.unwrap());
}
