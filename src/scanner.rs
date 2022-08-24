const OCB_CHR: u8 = b'{'; // Opening Curly Bracket
const CCB_CHR: u8 = b'}'; // Closing Curly Bracket
const OSB_CHR: u8 = b'['; // Opening Square Bracket
const CSB_CHR: u8 = b']'; // Closing Square Bracket
const DQ_CHR: u8 = b'"'; // Double Quotes
// Ignore
const SPACE_CHR: u8 = b' '; // Space
const TAP_CHR: u8 = b'\t';
const NEWLINE_CHR: u8 = b'\n';
const RETURN_CHR: u8 = b'\r';

const COMMA_CHR: u8 = b','; // Comma
const COLON_CHR: u8 = b':'; // Colon
const ESCAPE_CHR: u8 = b'\\'; // Escape

#[derive(Debug)]
pub struct Scanner {
    pub keys_to_remove: Vec<String>,
    pub next_buffer_index: usize,
    // State
    pub state: ScannerState,
    // Checker
    waiting_condition: WaitingCondition,
    key_cache: KeyCache,
    value_type_definer: ValueTypeDefiner,
    value_range_checker: ValueRangeChecker,
    pub skip_end_msg_cache: Message,
    // Message Queue
    pub queue: Vec<Message>,
}

#[derive(Debug)]
pub enum ScannerState {
    WaitingNextKey,
    MeetKeyCandOpener, // Meet '{' or '[' or ','
    ConfirmingKey, // Meet " next to CandSite
    DefiningValueType,
    CheckingValueRange,
    FindingNextComma,
}

pub type ChrIndex = (usize, usize); // (buffer index, character index)

#[derive(Debug, Clone)]
pub enum Message {
    SkipStartFrom(ChrIndex),
    SkipEndTo(ChrIndex),
    SkipEndPreviousTo(ChrIndex),
}

impl Default for Message {
    fn default() -> Self {
        Self::SkipStartFrom((0,0))
    }
}

impl Scanner {
    pub fn new(keys_to_remove: Vec<String>) -> Self {
        Self {
            keys_to_remove,
            next_buffer_index: 0,
            state: ScannerState::WaitingNextKey,
            waiting_condition: WaitingCondition::default(),
            key_cache: KeyCache::default(),
            value_type_definer: ValueTypeDefiner::default(),
            value_range_checker: ValueRangeChecker::default(),
            skip_end_msg_cache: Message::default(),
            queue: Vec::new(),
        }
    }
    pub fn process_new_buffer(&mut self, buffer: &[u8]) {
        // (1) Deal with each character
        buffer.iter().enumerate().for_each(|(pos, chr)| {
            match &self.state {
                ScannerState::WaitingNextKey => {
                    let meet_key_opener = self.waiting_condition.check_key_opener(chr);
                    if meet_key_opener {
                        self.waiting_condition.update_position((self.next_buffer_index, pos));
                        self.state = ScannerState::MeetKeyCandOpener;
                    }
                },
                ScannerState::MeetKeyCandOpener => {
                    match *chr {
                        SPACE_CHR | TAP_CHR | NEWLINE_CHR | RETURN_CHR => {
                            // pass
                        },
                        DQ_CHR => {
                            let new_key_cache = KeyCache::new((self.next_buffer_index, pos));
                            self.key_cache = new_key_cache;
                            self.state = ScannerState::ConfirmingKey;
                        },
                        _ => {
                            // Return to wait key
                            // In the case of
                            //  - {}
                            //  - []
                            //  - ...
                            self.state = ScannerState::WaitingNextKey;
                        },
                    }
                },
                ScannerState::ConfirmingKey => {
                    let confirmed = self.key_cache.confirm_key(chr);
                    if confirmed {
                        // (1) Check if key is to remove
                        let mut need_to_remove = false;
                        for string in &self.keys_to_remove {
                            if *string == self.key_cache.key_string {
                                need_to_remove = true;
                                break
                            }
                        };
                        // (2) Change state
                        if need_to_remove {
                            let skip_start_msg = if self.waiting_condition.key_cand_opener_is_comma() {
                                Message::SkipStartFrom(self.waiting_condition.opener_position)
                            } else {
                                Message::SkipStartFrom(self.key_cache.dq_start_position)
                            };
                            self.queue.push(skip_start_msg);
                            self.value_type_definer.init();
                            self.state = ScannerState::DefiningValueType;
                        } else {
                            // Return to wait key
                            self.state = ScannerState::WaitingNextKey;
                        }
                    }
                },
                ScannerState::DefiningValueType => {
                    let defined_value_type = self.value_type_definer.define_value_type(chr);

                    if let Some(value_type) = defined_value_type {
                        self.value_range_checker = ValueRangeChecker::new(value_type);
                        self.state = ScannerState::CheckingValueRange;
                    }
                },
                ScannerState::CheckingValueRange => {
                    let closing_condition = self.value_range_checker.check_meeting_closing(*chr);

                    if let ClosingCondition::ClosedWithComma(closed_with_comma) = closing_condition {
                        // (1) Get value end position
                        let value_is_to_previous = self.value_range_checker.range_is_to_previous_chr();
                        let skip_end_msg = if value_is_to_previous {
                            Message::SkipEndPreviousTo((self.next_buffer_index, pos))
                        } else {
                            Message::SkipEndTo((self.next_buffer_index, pos))
                        };
                        
                        // (2) Next state
                        let key_opener_is_comma = self.waiting_condition.key_cand_opener_is_comma();
                        if key_opener_is_comma {
                            if closed_with_comma {
                                self.queue.push(skip_end_msg);

                                self.waiting_condition.key_cand_opener = KeyCandOpener::MeetComma;
                                self.waiting_condition.update_position((self.next_buffer_index, pos));
                                self.state = ScannerState::MeetKeyCandOpener;
                            } else {
                                self.queue.push(skip_end_msg);

                                self.state = ScannerState::WaitingNextKey;
                            }
                        } else {
                            if closed_with_comma {
                                let skip_end_msg = Message::SkipEndTo((self.next_buffer_index, pos));
                                self.queue.push(skip_end_msg);

                                self.waiting_condition.key_cand_opener = KeyCandOpener::MeetNonComma;
                                self.waiting_condition.update_position((self.next_buffer_index, pos)); // Treat comma as non comma
                                self.state = ScannerState::MeetKeyCandOpener;
                            } else {
                                // End signal is deferred
                                self.skip_end_msg_cache = skip_end_msg;
                                self.state = ScannerState::FindingNextComma;
                            }
                        }
                    };
                },
                ScannerState::FindingNextComma => {
                    match *chr {
                        SPACE_CHR | TAP_CHR | NEWLINE_CHR | RETURN_CHR => {
                            // pass
                        },
                        COMMA_CHR => {
                            let skip_end_msg = Message::SkipEndTo((self.next_buffer_index, pos));
                            self.queue.push(skip_end_msg);

                            self.waiting_condition.key_cand_opener = KeyCandOpener::MeetNonComma;
                            self.waiting_condition.update_position((self.next_buffer_index, pos)); // Treat comma as non comma
                            self.state = ScannerState::MeetKeyCandOpener;
                        },
                        _ => {
                            self.queue.push(self.skip_end_msg_cache.clone());

                            // Act like in WaitingCondition state
                            let meet_key_opener = self.waiting_condition.check_key_opener(chr);
                            if meet_key_opener {
                                self.waiting_condition.update_position((self.next_buffer_index, pos));
                                self.state = ScannerState::MeetKeyCandOpener;
                            } else {
                                self.state = ScannerState::WaitingNextKey;
                            }
                        },
                    }
                },
            }
        });

        // (2) Increase index
        self.next_buffer_index += 1;
    }
    pub fn key_cand_opener_index(&self) -> ChrIndex {
        self.waiting_condition.opener_position
    }  
}

#[derive(Debug)]
struct WaitingCondition {
    key_cand_opener: KeyCandOpener,
    opener_position: ChrIndex,
}
#[derive(Debug)]
enum KeyCandOpener {
    MeetNonComma, // { or [
    MeetComma, // , " -> Start position is ,
}
impl Default for WaitingCondition {
    fn default() -> Self {
        Self {
            key_cand_opener: KeyCandOpener::MeetNonComma,
            opener_position: (0, 0),
        }
    }
}
impl WaitingCondition {
    fn check_key_opener(&mut self, chr: &u8) -> bool {
        match *chr {
            OCB_CHR | OSB_CHR => {
                self.key_cand_opener = KeyCandOpener::MeetNonComma;
                true
            },
            COMMA_CHR => {
                self.key_cand_opener = KeyCandOpener::MeetComma;
                true
            },
            _ => {
                false
            },
        }
    }
    fn update_position(&mut self, opener_position: ChrIndex) {
        self.opener_position = opener_position;
    }
    fn key_cand_opener_is_comma(&self) -> bool {
        match self.key_cand_opener {
            KeyCandOpener::MeetComma => true,
            _ => false,
        }
    }
}

#[derive(Debug)]
struct KeyCache {
    key_string: String,
    dq_start_position: ChrIndex,
    escape_next: bool,
}
impl Default for KeyCache {
    fn default() -> Self {
        Self::new((0,0))
    }
}
impl KeyCache {
    fn new(dq_position: ChrIndex) -> Self {
        Self {
            key_string: String::new(),
            dq_start_position: dq_position,
            escape_next: false,
        }
    }
    fn confirm_key(&mut self, chr: &u8) -> bool {
        if self.escape_next {
            // Push always
            self.key_string.push(*chr as char);
            self.escape_next = false;
            false
        } else {
            match *chr {
                ESCAPE_CHR => {
                    self.escape_next = true;
                    false
                },
                DQ_CHR => {
                    true
                },
                _ => {
                    // All other chr to key
                    self.key_string.push(*chr as char);
                    false
                },
            }
        }
    }
}

#[derive(Debug, Default)]
struct ValueTypeDefiner {
    meet_colon: bool,
}
impl ValueTypeDefiner {
    fn init(&mut self) {
        self.meet_colon = false;
    }
    fn define_value_type(&mut self, chr: &u8) -> Option<ValueType> {
        if self.meet_colon {
            match *chr {
                OCB_CHR => {
                    Some(ValueType::Object)
                },
                OSB_CHR => {
                    Some(ValueType::Array)
                },
                DQ_CHR => {
                    Some(ValueType::String)
                },
                SPACE_CHR | TAP_CHR | NEWLINE_CHR | RETURN_CHR => {
                    None
                },
                _ => {
                    Some(ValueType::Others)
                },
            }
        } else {
            // Waiting colon
            match *chr {
                SPACE_CHR | TAP_CHR | NEWLINE_CHR | RETURN_CHR => {
                    None
                },
                COLON_CHR => {
                    self.meet_colon = true;
                    None
                },
                _ => {
                    panic!("Colon is not right after the Key") // TODO: To debug, remove later
                }
            }
        }
    }
}

#[derive(Debug)]
struct ValueRangeChecker {
    value_type: ValueType,
    hierarchy: usize,
    escape_next: bool,
}
impl Default for ValueRangeChecker {
    fn default() -> Self {
        Self::new(ValueType::Object)
    }
}
impl ValueRangeChecker {
    fn new(value_type: ValueType) -> Self {
        Self {
            value_type,
            hierarchy: 0,
            escape_next: false,
        }
    }
    fn check_meeting_closing(&mut self, chr: u8) -> ClosingCondition {
        if !self.escape_next {
            match chr {
                // Curly Bracket
                OCB_CHR => {
                    match self.value_type {
                        ValueType::Object => {
                            self.hierarchy += 1;
                            ClosingCondition::ClosedYet
                        },
                        _ => {
                            ClosingCondition::ClosedYet
                        }
                    }
                },
                CCB_CHR => {
                    match self.value_type {
                        ValueType::Object => {
                            if self.hierarchy == 0 {
                                ClosingCondition::ClosedWithComma(false)
                            } else {
                                self.hierarchy -= 1;
                                ClosingCondition::ClosedYet
                            }
                        },
                        ValueType::Others => {
                            ClosingCondition::ClosedWithComma(false)
                        },
                        _ => {
                            ClosingCondition::ClosedYet
                        },
                    }
                },
                // Square Bracket
                OSB_CHR => {
                    match self.value_type {
                        ValueType::Array => {
                            self.hierarchy += 1;
                            ClosingCondition::ClosedYet
                        },
                        _ => {
                            ClosingCondition::ClosedYet
                        }
                    }
                },
                CSB_CHR => {
                    match self.value_type {
                        ValueType::Array => {
                            if self.hierarchy == 0 {
                                ClosingCondition::ClosedWithComma(false)
                            } else {
                                self.hierarchy -= 1;
                                ClosingCondition::ClosedYet
                            }
                        },
                        ValueType::Others => {
                            ClosingCondition::ClosedWithComma(false)
                        },
                        _ => {
                            ClosingCondition::ClosedYet
                        },
                    }
                },
                DQ_CHR => {
                    match self.value_type {
                        ValueType::String | ValueType::Others => {
                            ClosingCondition::ClosedWithComma(false)
                        },
                        _ => {
                            ClosingCondition::ClosedYet
                        },
                    }
                },
                // White space
                SPACE_CHR | TAP_CHR | NEWLINE_CHR | RETURN_CHR => {
                    match self.value_type {
                        ValueType::Others => {
                            ClosingCondition::ClosedWithComma(false)
                        },
                        _ => {
                            ClosingCondition::ClosedYet
                        },
                    }
                },
                // Comma
                COMMA_CHR => {
                    match self.value_type {
                        ValueType::Others => {
                            ClosingCondition::ClosedWithComma(true)
                        },
                        _ => {
                            ClosingCondition::ClosedYet
                        },
                    }
                },
                ESCAPE_CHR => {
                    self.escape_next = true;
                    ClosingCondition::ClosedYet
                },
                _ => {
                    ClosingCondition::ClosedYet
                }
            }
        } else {
            self.escape_next = false;
            ClosingCondition::ClosedYet
        }
    }
    fn range_is_to_previous_chr(&self) -> bool {
        match self.value_type {
            ValueType::Others => true,
            _ => false,
        }
    }
}

#[derive(Debug)]
enum ValueType {
    Object,
    String,
    Array,
    Others, // Null, Number, Bool
}

enum ClosingCondition {
    ClosedYet,
    ClosedWithComma(bool),
}
