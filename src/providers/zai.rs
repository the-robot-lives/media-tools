use std::path::Path;

use crate::attachments::LoadedAttachment;
use crate::providers::openai_chat::openai_compatible_generate;
use crate::providers::{ChatProvider, GenerationOptions};

pub struct ZaiProvider;

#[async_trait::async_trait]
impl ChatProvider for ZaiProvider {
    async fn generate(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        output_path: &Path,
        api_key: &str,
        options: &GenerationOptions,
        attachments: &[LoadedAttachment],
    ) -> color_eyre::Result<bool> {
        // General z.ai API-key base. z.ai also runs a separate coding-plan
        // endpoint (`https://api.z.ai/api/coding/paas/v4`, OpenAI-compatible)
        // for coding-subscription accounts — see tobor-kit's
        // `llm-inference/catalog.ts` `zai` entry, which defaults to that
        // coding-plan base because it targets coding-plan subscribers
        // specifically. That base is plausibly correct for a coding
        // subscription and wrong for a general ZAI_API_KEY, so we do not
        // default to it here and do not attempt to auto-detect which plan a
        // key belongs to. Coding-plan subscribers can override the base URL
        // per-provider once `provider_config`/`media-tool.yaml` grows a
        // `base_url` override knob (not present yet — see docs/providers.md).
        openai_compatible_generate(
            "https://api.z.ai/v1/chat/completions",
            system_prompt,
            user_prompt,
            output_path,
            api_key,
            options,
            attachments,
        )
        .await
    }

    fn name(&self) -> &str {
        "zai"
    }
}
