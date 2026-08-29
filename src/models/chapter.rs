use anyhow::{Result, bail, ensure};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{Block, BlockType};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChapterMeta {
    #[serde(default = "default_status")]
    pub status: String,
}

fn default_status() -> String {
    "draft".to_string()
}

impl Default for ChapterMeta {
    fn default() -> Self {
        Self {
            status: default_status(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chapter {
    pub schema_version: u32,
    pub id: Uuid,
    pub title: String,
    pub blocks: Vec<Block>,
    pub meta: ChapterMeta,
}

impl Chapter {
    pub fn new(title: impl Into<String>, now: DateTime<Utc>) -> Self {
        Self {
            schema_version: 1,
            id: Uuid::now_v7(),
            title: title.into(),
            blocks: vec![Block::new(BlockType::Narration, String::new(), now)],
            meta: ChapterMeta::default(),
        }
    }

    /// 在 `index` 处插入块（原 index 及之后后移）。
    pub fn insert_block(&mut self, index: usize, block: Block) -> Result<()> {
        ensure!(index <= self.blocks.len(), "insert index out of range");
        self.blocks.insert(index, block);
        Ok(())
    }

    /// 删除指定下标的块。
    pub fn remove_block(&mut self, index: usize) -> Result<Block> {
        ensure!(index < self.blocks.len(), "remove index out of range");
        Ok(self.blocks.remove(index))
    }

    /// 切换块类型；非 dialogue 清除 speaker。
    pub fn set_block_type(
        &mut self,
        index: usize,
        new_type: BlockType,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let block = self
            .blocks
            .get_mut(index)
            .ok_or_else(|| anyhow::anyhow!("block index out of range"))?;
        block.block_type = new_type;
        if new_type != BlockType::Dialogue {
            block.speaker = None;
        }
        block.meta.updated_at = now;
        Ok(())
    }

    /// 设置 dialogue 的 speaker（非 dialogue 报错）。
    pub fn set_speaker(
        &mut self,
        index: usize,
        speaker: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let block = self
            .blocks
            .get_mut(index)
            .ok_or_else(|| anyhow::anyhow!("block index out of range"))?;
        ensure!(
            block.block_type == BlockType::Dialogue,
            "speaker only valid on dialogue blocks"
        );
        block.speaker = speaker;
        block.meta.updated_at = now;
        Ok(())
    }

    /// 更新正文内容。
    pub fn set_block_content(
        &mut self,
        index: usize,
        content: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let block = self
            .blocks
            .get_mut(index)
            .ok_or_else(|| anyhow::anyhow!("block index out of range"))?;
        block.content = content.into();
        block.meta.updated_at = now;
        Ok(())
    }

    /// 在光标字节偏移处分割；之后新建 narration 块。
    pub fn split_block_at(
        &mut self,
        index: usize,
        byte_offset: usize,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let block = self
            .blocks
            .get_mut(index)
            .ok_or_else(|| anyhow::anyhow!("block index out of range"))?;
        ensure!(
            byte_offset <= block.content.len() && block.content.is_char_boundary(byte_offset),
            "split offset must be on a char boundary"
        );
        let after = block.content[byte_offset..].to_string();
        block.content.truncate(byte_offset);
        block.meta.updated_at = now;
        let new_block = Block::new(BlockType::Narration, after, now);
        self.blocks.insert(index + 1, new_block);
        Ok(())
    }

    /// 合并连续文本块 `[start, end]`（含端点）。
    ///
    /// - 不允许包含 `scene_break`
    /// - 类型取首块；正文换行连接
    /// - 首块非 dialogue 则清 speaker，否则保留首块 speaker
    pub fn merge_blocks(&mut self, start: usize, end: usize, now: DateTime<Utc>) -> Result<()> {
        ensure!(end < self.blocks.len(), "merge end out of range");
        ensure!(end > start, "merge requires at least two blocks");
        for i in start..=end {
            if self.blocks[i].block_type == BlockType::SceneBreak {
                bail!("scene_break cannot be merged with text blocks");
            }
        }

        let first_type = self.blocks[start].block_type;
        let speaker = if first_type == BlockType::Dialogue {
            self.blocks[start].speaker.clone()
        } else {
            None
        };
        let content = self.blocks[start..=end]
            .iter()
            .map(|b| b.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        let first = &mut self.blocks[start];
        first.block_type = first_type;
        first.content = content;
        first.speaker = speaker;
        first.meta.updated_at = now;
        self.blocks.drain(start + 1..=end);
        Ok(())
    }

    /// 将 `from` 块移动到 `to`（移除后目标下标）。
    pub fn move_block(&mut self, from: usize, to: usize) -> Result<()> {
        ensure!(from < self.blocks.len(), "move from out of range");
        ensure!(to <= self.blocks.len(), "move to out of range");
        if from == to || from + 1 == to {
            return Ok(());
        }
        let block = self.blocks.remove(from);
        let insert_at = if to > from { to - 1 } else { to };
        self.blocks.insert(insert_at, block);
        Ok(())
    }

    /// 与相邻块交换（上移/下移），拖拽排序的替代。
    pub fn swap_block(&mut self, index: usize, up: bool) -> Result<()> {
        if up {
            ensure!(index > 0, "already at top");
            self.blocks.swap(index - 1, index);
        } else {
            ensure!(index + 1 < self.blocks.len(), "already at bottom");
            self.blocks.swap(index, index + 1);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 30, 1, 0, 0).unwrap()
    }

    fn chapter_with(blocks: Vec<Block>) -> Chapter {
        let mut ch = Chapter::new("测", now());
        ch.blocks = blocks;
        ch
    }

    #[test]
    fn chapter_roundtrip_json() {
        let chapter = Chapter::new("第一章 开篇", now());
        let json = serde_json::to_string_pretty(&chapter).unwrap();
        assert!(json.contains(r#""schema_version": 1"#));
        assert!(json.contains(r#""status": "draft""#));

        let parsed: Chapter = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, chapter);
        assert_eq!(parsed.schema_version, 1);
        assert_eq!(parsed.title, "第一章 开篇");
        assert_eq!(parsed.blocks.len(), 1);
        assert_eq!(parsed.meta.status, "draft");
    }

    #[test]
    fn insert_and_remove_block() {
        let mut ch = Chapter::new("测", now());
        let b = Block::new(BlockType::Dialogue, "你好", now());
        ch.insert_block(1, b).unwrap();
        assert_eq!(ch.blocks.len(), 2);
        assert_eq!(ch.blocks[1].block_type, BlockType::Dialogue);
        let removed = ch.remove_block(0).unwrap();
        assert_eq!(removed.block_type, BlockType::Narration);
        assert_eq!(ch.blocks.len(), 1);
    }

    #[test]
    fn set_block_type_clears_speaker_unless_dialogue() {
        let mut ch = chapter_with(vec![Block::new_dialogue("话", "甲", now())]);
        ch.set_block_type(0, BlockType::Narration, now()).unwrap();
        assert_eq!(ch.blocks[0].block_type, BlockType::Narration);
        assert!(ch.blocks[0].speaker.is_none());

        ch.set_block_type(0, BlockType::Dialogue, now()).unwrap();
        ch.set_speaker(0, Some("乙".into()), now()).unwrap();
        assert_eq!(ch.blocks[0].speaker.as_deref(), Some("乙"));
    }

    #[test]
    fn split_block_at_cursor_creates_narration() {
        let mut ch = chapter_with(vec![Block::new(BlockType::Dialogue, "前半后半", now())]);
        // "前半" is 6 bytes in UTF-8
        let offset = "前半".len();
        ch.split_block_at(0, offset, now()).unwrap();
        assert_eq!(ch.blocks.len(), 2);
        assert_eq!(ch.blocks[0].content, "前半");
        assert_eq!(ch.blocks[0].block_type, BlockType::Dialogue);
        assert_eq!(ch.blocks[1].content, "后半");
        assert_eq!(ch.blocks[1].block_type, BlockType::Narration);
    }

    #[test]
    fn merge_consecutive_text_blocks() {
        let mut ch = chapter_with(vec![
            Block::new_dialogue("一", "甲", now()),
            Block::new(BlockType::Narration, "二", now()),
            Block::new(BlockType::Note, "三", now()),
        ]);
        ch.merge_blocks(0, 2, now()).unwrap();
        assert_eq!(ch.blocks.len(), 1);
        assert_eq!(ch.blocks[0].block_type, BlockType::Dialogue);
        assert_eq!(ch.blocks[0].content, "一\n二\n三");
        assert_eq!(ch.blocks[0].speaker.as_deref(), Some("甲"));
    }

    #[test]
    fn merge_non_dialogue_first_clears_speaker() {
        let mut ch = chapter_with(vec![
            Block::new(BlockType::Narration, "甲段", now()),
            Block::new_dialogue("乙话", "乙", now()),
        ]);
        ch.merge_blocks(0, 1, now()).unwrap();
        assert_eq!(ch.blocks[0].block_type, BlockType::Narration);
        assert!(ch.blocks[0].speaker.is_none());
        assert_eq!(ch.blocks[0].content, "甲段\n乙话");
    }

    #[test]
    fn merge_rejects_scene_break() {
        let mut ch = chapter_with(vec![
            Block::new(BlockType::Narration, "a", now()),
            Block::new(BlockType::SceneBreak, "***", now()),
        ]);
        let err = ch.merge_blocks(0, 1, now()).unwrap_err();
        assert!(err.to_string().contains("scene_break"));
    }

    #[test]
    fn move_and_swap_reorder() {
        let mut ch = chapter_with(vec![
            Block::new(BlockType::Narration, "a", now()),
            Block::new(BlockType::Narration, "b", now()),
            Block::new(BlockType::Narration, "c", now()),
        ]);
        ch.swap_block(2, true).unwrap();
        assert_eq!(
            ch.blocks
                .iter()
                .map(|b| b.content.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "c", "b"]
        );
        ch.move_block(2, 0).unwrap();
        assert_eq!(
            ch.blocks
                .iter()
                .map(|b| b.content.as_str())
                .collect::<Vec<_>>(),
            vec!["b", "a", "c"]
        );
    }
}
