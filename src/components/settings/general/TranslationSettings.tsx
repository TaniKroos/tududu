import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  commands,
  type ModelInfo,
  type TranslationEnglishCopy,
  type TranslationOptions,
} from "@/bindings";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { SettingContainer } from "../../ui/SettingContainer";
import { Dropdown } from "../../ui/Dropdown";
import { Alert } from "../../ui/Alert";
import { ShortcutInput } from "../ShortcutInput";
import { useSettings } from "../../../hooks/useSettings";
import { useModelStore } from "../../../stores/modelStore";
import { getLanguageLabel } from "../../../lib/constants/languages";

const languageName = (code: string) => getLanguageLabel(code) ?? code;

/**
 * The translate shortcut and the language it translates into. The choices
 * come from the selected model's own file: the languages it can translate the
 * current source language into.
 */
export const TranslationSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const { currentModel, models } = useModelStore();
  const [options, setOptions] = useState<TranslationOptions | null>(null);

  const target = getSetting("translation_target_language") ?? null;
  const sourceIntent = getSetting("selected_language") || "auto";
  const modelInfo = models.find((m: ModelInfo) => m.id === currentModel);

  useEffect(() => {
    if (!currentModel || !modelInfo?.is_downloaded) {
      setOptions(null);
      return;
    }
    let cancelled = false;
    commands.getTranslationOptions(currentModel).then((result) => {
      if (!cancelled) setOptions(result.status === "ok" ? result.data : null);
    });
    return () => {
      cancelled = true;
    };
    // The source language and the model's runtime capabilities both change
    // which targets are reachable.
  }, [
    currentModel,
    sourceIntent,
    modelInfo?.is_downloaded,
    modelInfo?.supports_translation,
  ]);

  const targetOptions = (options?.targets ?? []).map((code) => ({
    value: code,
    label: languageName(code),
  }));
  const targetReachable =
    target !== null && targetOptions.some((o) => o.value === target);
  const modelName = modelInfo?.name ?? "";
  const englishCopy = getSetting("translation_english_copy") ?? "off";
  const englishCopyOptions: { value: TranslationEnglishCopy; label: string }[] =
    [
      { value: "off", label: t("settings.translation.englishCopy.off") },
      {
        value: "new_line",
        label: t("settings.translation.englishCopy.newLine"),
      },
      {
        value: "brackets",
        label: t("settings.translation.englishCopy.brackets"),
      },
    ];

  return (
    <SettingsGroup title={t("settings.translation.title")}>
      <ShortcutInput shortcutId="transcribe_translate" grouped={true} />
      <SettingContainer
        title={t("settings.translation.target.title")}
        description={t("settings.translation.target.description")}
        descriptionMode="tooltip"
        grouped={true}
      >
        <Dropdown
          options={targetOptions}
          selectedValue={targetReachable ? target : null}
          onSelect={(value) =>
            updateSetting("translation_target_language", value)
          }
          placeholder={t("settings.translation.target.placeholder")}
          disabled={
            targetOptions.length === 0 ||
            isUpdating("translation_target_language")
          }
        />
      </SettingContainer>
      <SettingContainer
        title={t("settings.translation.englishCopy.title")}
        description={t("settings.translation.englishCopy.description")}
        descriptionMode="tooltip"
        grouped={true}
      >
        <Dropdown
          options={englishCopyOptions}
          selectedValue={englishCopy}
          onSelect={(value) =>
            updateSetting(
              "translation_english_copy",
              value as TranslationEnglishCopy,
            )
          }
          disabled={isUpdating("translation_english_copy")}
        />
      </SettingContainer>
      {options && !options.supported && (
        <Alert variant="warning" contained>
          {t("settings.translation.unsupportedModel", { model: modelName })}
        </Alert>
      )}
      {options?.supported && target !== null && !targetReachable && (
        <Alert variant="warning" contained>
          {t("settings.translation.unreachableTarget", {
            model: modelName,
            source: languageName(options.source_language),
            target: languageName(target),
          })}
        </Alert>
      )}
      {options?.supported && target === null && (
        <Alert variant="info" contained>
          {t("settings.translation.chooseLanguage")}
        </Alert>
      )}
    </SettingsGroup>
  );
};
