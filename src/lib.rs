use std::io::{Read, Write};

mod scanner;
use scanner::{
    Scanner,
    ScannerState,
    ChrIndex, Message,
};

#[derive(Debug)]
pub struct KeyRemover {
    scanner: Scanner,
    buffer_size: usize,
    first_buffer_index: usize,
    buffer_queue: Vec<Vec<u8>>,
    buffer_length_queue: Vec<usize>,
    message_queue: Vec<Message>,
}

impl KeyRemover {
    pub fn init(
        buffer_size: usize,
        keys_to_remove: Vec<String>,
    ) -> Self {
        let scanner = Scanner::new(keys_to_remove);

        Self {
            scanner,
            buffer_size,
            first_buffer_index: 0,
            buffer_queue: Vec::new(),
            buffer_length_queue: Vec::new(),
            message_queue: Vec::new(),
        }
    }
    pub fn process<R, W>(&mut self, mut reader: R, mut writer: W) where
        R: Read, W: Write,
    {
        let mut mode = Mode::Remain;

        // (1) Read first buffer
        let mut first_buffer = vec![0; self.buffer_size];
        let mut filled_byte_size = reader.read(&mut first_buffer).unwrap(); // TODO: Deal error
        self.scanner.process_new_buffer(&first_buffer[..filled_byte_size]);
        self.buffer_queue.push(first_buffer);
        self.buffer_length_queue.push(filled_byte_size);

        // (2) While file end
        while filled_byte_size != 0 { // TODO: Handle with slow stream
            // (1) Pull out messages
            self.message_queue.append(&mut self.scanner.queue);

            // (2) Get range of data to write
            let optional_data_index_to_write = match self.scanner.state {
                ScannerState::WaitingNextKey => {
                    // Write all message and buffer
                    let last_buffer_index_to_write = self.scanner.next_buffer_index - 1;
                    let message_end_index = self.message_queue.len();

                    Some((last_buffer_index_to_write, message_end_index))
                },
                ScannerState::CheckingValueRange => {
                    if self.scanner.next_buffer_index == 1 {
                        None
                    } else {
                        let last_buffer_index_to_write = self.scanner.next_buffer_index - 2;
                        if last_buffer_index_to_write < self.first_buffer_index {
                            None
                        } else {
                            let message_end_index = self.get_message_end_index(last_buffer_index_to_write);
                            Some((last_buffer_index_to_write, message_end_index))
                        }
                    }
                },
                ScannerState::FindingNextComma => {
                    let cached_skip_end_msg = &self.scanner.skip_end_msg_cache;
                    let last_buffer_index_to_write = match self.get_optional_skip_end_index(cached_skip_end_msg) {
                        Some(chr_index) => chr_index.0 - 1,
                        None => panic!("Error 1"), // TODO: Unreachable error msg
                    };
                    if last_buffer_index_to_write < self.first_buffer_index {
                        None
                    } else {
                        let message_end_index = self.get_message_end_index(last_buffer_index_to_write);
                        Some((last_buffer_index_to_write, message_end_index))
                    }
                },
                _ => {
                    // ScannerState::MeetKeyCandOpener | ScannerState::ConfirmingKey | ScannerState::DefiningValueType
                    // Defer
                    let chr_index = self.previous_chr_index(&self.scanner.key_cand_opener_index());
                    if self.first_buffer_index < chr_index.0 {
                        let last_buffer_index_to_write = chr_index.0 - 1;
                        let message_end_index = self.get_message_end_index(last_buffer_index_to_write);
                        Some((last_buffer_index_to_write, message_end_index))
                    }else {
                        None
                    }
                },
            };

            // (3) Write data
            if let Some((last_buffer_index_to_write, message_end_index)) = optional_data_index_to_write {
                // (1) Get messages
                let mut messages_to_write: Vec<Message> = self.message_queue.drain(0..message_end_index).collect();

                // (2) Add messages by mode
                if let Mode::Skip = mode {
                    messages_to_write.insert(0, Message::SkipStartFrom((self.first_buffer_index, 0)));
                }
                if let Some(Message::SkipStartFrom(_)) = messages_to_write.last() {
                    messages_to_write.push(Message::SkipEndTo((
                        last_buffer_index_to_write,
                        self.buffer_length_queue[last_buffer_index_to_write-self.first_buffer_index]-1
                    )));
                    mode = Mode::Skip;
                } else {
                    mode = Mode::Remain;
                }

                // (3) Transform buffers to write
                messages_to_write.chunks(2).rev().for_each(|messages| {
                    let skip_start_chr_index = match messages[0] {
                        Message::SkipStartFrom(chr_index) => {
                            (chr_index.0 - self.first_buffer_index, chr_index.1)
                        },
                        _ => panic!("Error 2"),
                    };
                    let skip_end_chr_index = match messages[1] {
                        Message::SkipEndTo(chr_index) => {
                            (chr_index.0 - self.first_buffer_index, chr_index.1)
                        },
                        Message::SkipEndPreviousTo(chr_index) => {
                            let chr_index = self.previous_chr_index(&chr_index);
                            (chr_index.0 - self.first_buffer_index, chr_index.1)
                        },
                        _ => panic!("Error 3"),
                    };

                    if skip_start_chr_index.0 < skip_end_chr_index.0 {
                        let buffer_index = skip_end_chr_index.0;
                        let buffer = &mut self.buffer_queue[buffer_index];
                        let buffer_length = &mut self.buffer_length_queue[buffer_index];
                        buffer.drain(..=skip_end_chr_index.1);
                        *buffer_length -= skip_end_chr_index.1 + 1;
                        // middle buffer
                        for buffer_index in skip_start_chr_index.0+1..skip_end_chr_index.0 {
                            let buffer = &mut self.buffer_queue[buffer_index];
                            let buffer_length = &mut self.buffer_length_queue[buffer_index];
                            buffer.clear();
                            *buffer_length = 0;
                        }
                        // first buffer
                        let buffer_index = skip_start_chr_index.0;
                        let buffer = &mut self.buffer_queue[buffer_index];
                        let buffer_length = &mut self.buffer_length_queue[buffer_index];
                        buffer.drain(skip_start_chr_index.1..);
                        *buffer_length = skip_start_chr_index.1;
                    } else { // if skip_start_chr == skip_end_chr
                        let buffer_index = skip_start_chr_index.0;
                        let buffer = &mut self.buffer_queue[buffer_index];
                        let buffer_length = &mut self.buffer_length_queue[buffer_index];
                        buffer.drain(skip_start_chr_index.1..=skip_end_chr_index.1);
                        *buffer_length -= skip_end_chr_index.1 - skip_start_chr_index.1 + 1;
                    }
                });

                // (3) Write buffers
                let count_of_buffer_to_write = last_buffer_index_to_write - self.first_buffer_index + 1;
                for buffer_index in 0..count_of_buffer_to_write {
                    let buffer = &self.buffer_queue[buffer_index];
                    let length = self.buffer_length_queue[buffer_index];
                    writer.write_all(&buffer[..length]).unwrap();
                };
                self.buffer_queue.drain(..count_of_buffer_to_write);
                self.buffer_length_queue.drain(..count_of_buffer_to_write);
                self.first_buffer_index += count_of_buffer_to_write;
            }

            // (4) Load next buffer
            let mut next_buffer = vec![0; self.buffer_size];
            filled_byte_size = reader.read(&mut next_buffer).unwrap(); // TODO: Deal error
            self.buffer_length_queue.push(filled_byte_size);
            self.scanner.process_new_buffer(&next_buffer[..filled_byte_size]);
            self.buffer_queue.push(next_buffer);
        }

        writer.flush().unwrap();
    }
    fn get_message_end_index(&self, last_buffer_index_to_write: usize) -> usize {
        let mut message_end_index = 0;
        for message in self.message_queue.iter() {
            let chr_index = match &message {
                Message::SkipStartFrom(v) => v,
                Message::SkipEndTo(v) => v,
                Message::SkipEndPreviousTo(v) => v,
            };
            if chr_index.0 > last_buffer_index_to_write {
                break
            }
            message_end_index += 1;
        }
        message_end_index
    }
    fn get_optional_skip_end_index(&self, message: &Message) -> Option<ChrIndex> {
        match message {
            Message::SkipStartFrom(_) => {
                None
            },
            Message::SkipEndTo(chr_index) => {
                Some(chr_index.clone())
            },
            Message::SkipEndPreviousTo(chr_index) => {
                Some(self.previous_chr_index(chr_index))
            },
        }
    }
    fn previous_chr_index(&self, chr_index: &ChrIndex) -> ChrIndex {
        if chr_index.1 == 0 {
            let previous_buffer_index = chr_index.0 - 1;
            let buffer_queue_index = previous_buffer_index - self.first_buffer_index;
            (chr_index.0-1, self.buffer_length_queue[buffer_queue_index] - 1)
        } else {
            (chr_index.0, chr_index.1-1)
        }
    }
}

#[derive(Debug)]
enum Mode {
    Remain,
    Skip,
}

#[cfg(test)]
#[allow(dead_code)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn get_sample_json_string() -> String {
        let str = "{
            \"key_0\": {
              \"key_0_0\": \"val_0_0\",
              \"key_0_1\": \"val_0_1\",
              \"key_0_2\": {
                \"key_0_2_0\": [
                  {
                    \"key_0_2_0_0\": \"val_0_2_0_0\",
                    \"key_0_2_0_1\": \"val_0_2_0_1\"
                  },
                  {
                    \"key_0_2_1_0\": \"val_0_2_1_0\",
                    \"key_0_2_1_1\": \"val_0_2_1_1\"
                  },
                  {
                    \"key_0_2_2_0\": \"val_0_2_2_0\",
                    \"key_0_2_2_1\": \"val_0_2_2_1\"
                  }
                ]
              }
            }
          }";
        
        str.to_string()
    }

    fn print_sample_json() {
        let sample_json_string = get_sample_json_string();
        for (idx, string) in sample_json_string.as_bytes().iter().enumerate() {
            println!("# {}: {}", idx, String::from_utf8([*string].to_vec()).unwrap());
        }
    }

    #[test]
    fn test_1() {
        let sample_json_string = get_sample_json_string();
        let cloned_input_string = sample_json_string.clone();
        let mut key_remover = KeyRemover::init(128, vec!["key_0_2_0".to_string()]);

        let input = Cursor::new(sample_json_string);
        let mut output = Vec::new();
        key_remover.process(input, &mut output);

        println!("# Input");
        println!("{:?}", cloned_input_string);
        println!("# Output");
        println!("{:?}", String::from_utf8(output).unwrap());
    }
}