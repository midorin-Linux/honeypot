use anyhow::Result;
use serenity::{
    all::{
        CommandInteraction, CommandOptionType, CreateCommand, CreateCommandOption, CreateEmbed,
        CreateInteractionResponse, CreateInteractionResponseMessage, Permissions, ResolvedOption,
        ResolvedValue,
    },
    prelude::*,
};

use crate::{
    config::Config,
    db::{Sqlite, models::BanTriggerSettings},
};

/// `/config`スラッシュコマンドの定義。ギルドスコープで登録する(グローバル登録は反映に
/// 最大1時間かかるため採用しない)。閾値等はスパム対策の設定であるため、管理者または
/// メンバーBAN権限を持つユーザーのみに実行を許可する。
pub fn register() -> CreateCommand {
    CreateCommand::new("config")
        .description("ハニーポットのギルド別BAN判定設定を表示・変更します")
        .default_member_permissions(Permissions::ADMINISTRATOR | Permissions::BAN_MEMBERS)
        .add_option(CreateCommandOption::new(
            CommandOptionType::SubCommand,
            "show",
            "現在のBAN判定設定を表示します",
        ))
        .add_option(
            CreateCommandOption::new(
                CommandOptionType::SubCommand,
                "set",
                "BAN判定設定を変更します",
            )
            .add_sub_option(
                CreateCommandOption::new(CommandOptionType::String, "key", "変更する設定項目")
                    .required(true)
                    .add_string_choice("has_invite_link", "has_invite_link")
                    .add_string_choice("has_role_mention", "has_role_mention")
                    .add_string_choice("mention_threshold", "mention_threshold"),
            )
            .add_sub_option(
                CreateCommandOption::new(
                    CommandOptionType::String,
                    "value",
                    "設定する値（bool項目はtrue/false、mention_thresholdは数値）",
                )
                .required(true),
            ),
        )
        .add_option(CreateCommandOption::new(
            CommandOptionType::SubCommand,
            "reset",
            "BAN判定設定をデフォルト値にリセットします",
        ))
}

/// `/config`コマンドのハンドラ。DBはギルド別設定の永続化のみに責務を持ち、
/// BAN判定ロジック自体は`moderation`層に閉じる。
pub async fn handle(
    ctx: &Context,
    command: &CommandInteraction,
    db: &Sqlite,
    config: &Config,
) -> Result<()> {
    let Some(guild_id) = command.guild_id else {
        respond_ephemeral(ctx, command, "このコマンドはサーバー内でのみ使用できます。").await?;
        return Ok(());
    };

    let options = command.data.options();
    let Some(sub) = options.first() else {
        respond_ephemeral(ctx, command, "サブコマンドを指定してください。").await?;
        return Ok(());
    };

    match (sub.name, &sub.value) {
        ("show", ResolvedValue::SubCommand(_)) => {
            handle_show(ctx, command, db, config, guild_id.get()).await
        }
        ("set", ResolvedValue::SubCommand(sub_options)) => {
            handle_set(ctx, command, db, config, guild_id.get(), sub_options).await
        }
        ("reset", ResolvedValue::SubCommand(_)) => {
            handle_reset(ctx, command, db, config, guild_id.get()).await
        }
        _ => {
            respond_ephemeral(ctx, command, "不明なサブコマンドです。").await?;
            Ok(())
        }
    }
}

async fn handle_show(
    ctx: &Context,
    command: &CommandInteraction,
    db: &Sqlite,
    config: &Config,
    guild_id: u64,
) -> Result<()> {
    let stored = db.guild_config().await.get(guild_id).await?;
    let (settings, using_default) = match stored {
        Some(settings) => (settings, false),
        None => (BanTriggerSettings::from(&config.app.ban_trigger), true),
    };

    let embed = settings_embed("現在のBAN判定設定", &settings, using_default);
    respond_embed(ctx, command, embed).await
}

async fn handle_set(
    ctx: &Context,
    command: &CommandInteraction,
    db: &Sqlite,
    config: &Config,
    guild_id: u64,
    sub_options: &[ResolvedOption<'_>],
) -> Result<()> {
    let mut key = None;
    let mut value = None;

    for option in sub_options {
        match (option.name, &option.value) {
            ("key", ResolvedValue::String(v)) => key = Some(*v),
            ("value", ResolvedValue::String(v)) => value = Some(*v),
            _ => {}
        }
    }

    let (Some(key), Some(value)) = (key, value) else {
        respond_ephemeral(ctx, command, "key・valueの両方を指定してください。").await?;
        return Ok(());
    };

    let current = db
        .guild_config()
        .await
        .get(guild_id)
        .await?
        .unwrap_or_else(|| BanTriggerSettings::from(&config.app.ban_trigger));

    let updated = match key {
        "has_invite_link" => match parse_bool(value) {
            Some(v) => BanTriggerSettings {
                has_invite_link: v,
                ..current
            },
            None => {
                respond_ephemeral(
                    ctx,
                    command,
                    "has_invite_linkにはtrueまたはfalseを指定してください。",
                )
                .await?;
                return Ok(());
            }
        },
        "has_role_mention" => match parse_bool(value) {
            Some(v) => BanTriggerSettings {
                has_role_mention: v,
                ..current
            },
            None => {
                respond_ephemeral(
                    ctx,
                    command,
                    "has_role_mentionにはtrueまたはfalseを指定してください。",
                )
                .await?;
                return Ok(());
            }
        },
        "mention_threshold" => match value.parse::<u64>() {
            Ok(v) => BanTriggerSettings {
                mention_threshold: v,
                ..current
            },
            Err(_) => {
                respond_ephemeral(
                    ctx,
                    command,
                    "mention_thresholdには0以上の整数を指定してください。",
                )
                .await?;
                return Ok(());
            }
        },
        _ => {
            respond_ephemeral(ctx, command, "不明な設定項目です。").await?;
            return Ok(());
        }
    };

    db.guild_config().await.upsert(guild_id, &updated).await?;

    let embed = settings_embed("設定を更新しました", &updated, false);
    respond_embed(ctx, command, embed).await
}

async fn handle_reset(
    ctx: &Context,
    command: &CommandInteraction,
    db: &Sqlite,
    config: &Config,
    guild_id: u64,
) -> Result<()> {
    db.guild_config().await.reset(guild_id).await?;

    // リセット後の実効値は「行なし = YAMLフォールバック」なので、showと同じ値を表示する。
    let embed = settings_embed(
        "設定をデフォルト値にリセットしました",
        &BanTriggerSettings::from(&config.app.ban_trigger),
        true,
    );
    respond_embed(ctx, command, embed).await
}

fn parse_bool(value: &str) -> Option<bool> {
    match value {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn settings_embed(title: &str, settings: &BanTriggerSettings, using_default: bool) -> CreateEmbed {
    let mut embed = CreateEmbed::new()
        .title(title)
        .field(
            "招待リンク検知(has_invite_link)",
            settings.has_invite_link.to_string(),
            false,
        )
        .field(
            "ロールメンション検知(has_role_mention)",
            settings.has_role_mention.to_string(),
            false,
        )
        .field(
            "メンション数閾値(mention_threshold)",
            settings.mention_threshold.to_string(),
            false,
        );

    if using_default {
        embed = embed.description("※ ギルド別設定は未保存のため、デフォルト値を表示しています。");
    }

    embed
}

async fn respond_embed(
    ctx: &Context,
    command: &CommandInteraction,
    embed: CreateEmbed,
) -> Result<()> {
    let message = CreateInteractionResponseMessage::new()
        .embed(embed)
        .ephemeral(true);

    command
        .create_response(&ctx.http, CreateInteractionResponse::Message(message))
        .await?;

    Ok(())
}

async fn respond_ephemeral(ctx: &Context, command: &CommandInteraction, text: &str) -> Result<()> {
    let message = CreateInteractionResponseMessage::new()
        .content(text)
        .ephemeral(true);

    command
        .create_response(&ctx.http, CreateInteractionResponse::Message(message))
        .await?;

    Ok(())
}
