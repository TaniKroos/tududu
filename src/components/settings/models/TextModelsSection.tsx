import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { ask } from "@tauri-apps/plugin-dialog";
import { Check, Download, HardDrive, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { commands, type TextModelInfo } from "@/bindings";
import { useModelStore } from "@/stores/modelStore";
import Badge from "../../ui/Badge";
import { Button } from "../../ui/Button";
import { formatModelSize } from "../../../lib/utils/format";

/**
 * Text models: downloaded like speech models, but used for text tasks such as
 * translating selected text into English. They never appear in the speech
 * model list. Download progress comes from the shared model store, which hears
 * the same download events.
 */
export const TextModelsSection: React.FC = () => {
  const { t } = useTranslation();
  const { downloadProgress, downloadStats, verifyingModels } = useModelStore();
  const [models, setModels] = useState<TextModelInfo[]>([]);
  const [downloading, setDownloading] = useState<Record<string, true>>({});

  const refresh = useCallback(async () => {
    setModels(await commands.getTextModels());
  }, []);

  useEffect(() => {
    refresh();
    const unlisteners = [
      "model-download-complete",
      "model-download-failed",
      "model-download-cancelled",
    ].map((event) => listen(event, () => refresh()));
    return () => {
      unlisteners.forEach((unlisten) => unlisten.then((fn) => fn()));
    };
  }, [refresh]);

  const handleDownload = async (id: string) => {
    setDownloading((d) => ({ ...d, [id]: true }));
    // Failures are reported by the store's model-download-failed toast.
    await commands.downloadTextModel(id);
    setDownloading((d) => {
      const next = { ...d };
      delete next[id];
      return next;
    });
    refresh();
  };

  const handleDelete = async (model: TextModelInfo) => {
    const confirmed = await ask(
      t("settings.models.textModels.deleteConfirm", { name: model.name }),
      { title: t("settings.models.textModels.deleteTitle"), kind: "warning" },
    );
    if (!confirmed) return;
    const result = await commands.deleteTextModel(model.id);
    if (result.status === "error") toast.error(result.error);
    refresh();
  };

  if (models.length === 0) return null;

  return (
    <div className="space-y-3">
      <div>
        <h2 className="text-sm font-medium text-text/60">
          {t("settings.models.textModels.title")}
        </h2>
        <p className="text-xs text-text/50">
          {t("settings.models.textModels.description")}
        </p>
      </div>
      {models.map((model) => {
        const isDownloading = model.is_downloading || downloading[model.id];
        const progress = downloadProgress[model.id]?.percentage;
        const speed = downloadStats[model.id]?.speed;
        const verifying = verifyingModels[model.id];
        return (
          <div
            key={model.id}
            className={`flex flex-col rounded-xl px-4 py-3 gap-2 text-left border-2 ${
              model.is_downloaded
                ? "border-logo-primary/50 bg-logo-primary/10"
                : "border-mid-gray/20"
            }`}
          >
            <div className="flex items-center gap-3 flex-wrap">
              <h3 className="text-base font-semibold text-text">
                {model.name}
              </h3>
              {model.is_downloaded && (
                <Badge variant="primary">
                  <Check className="w-3 h-3 mr-1" />
                  {t("settings.models.textModels.ready")}
                </Badge>
              )}
            </div>
            <p className="text-text/60 text-sm leading-relaxed">
              {model.description}
            </p>
            <hr className="w-full border-mid-gray/20" />
            <div className="flex items-center gap-3 w-full h-5">
              <span className="flex items-center gap-1.5 text-xs text-text/50">
                {model.is_downloaded ? (
                  <HardDrive className="w-3.5 h-3.5" />
                ) : (
                  <Download className="w-3.5 h-3.5" />
                )}
                <span>{formatModelSize(model.size_mb)}</span>
              </span>
              <div className="ms-auto">
                {model.is_downloaded ? (
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => handleDelete(model)}
                    className="flex items-center gap-1.5 text-logo-primary/85 hover:text-logo-primary hover:bg-logo-primary/10"
                  >
                    <Trash2 className="w-3.5 h-3.5" />
                    <span>{t("common.delete")}</span>
                  </Button>
                ) : isDownloading ? (
                  <Button
                    variant="danger-ghost"
                    size="sm"
                    onClick={() => commands.cancelTextModelDownload(model.id)}
                  >
                    {t("modelSelector.cancel")}
                  </Button>
                ) : (
                  <Button
                    variant="primary"
                    size="sm"
                    onClick={() => handleDownload(model.id)}
                  >
                    {t("settings.models.textModels.download")}
                  </Button>
                )}
              </div>
            </div>
            {isDownloading && !verifying && (
              <div className="w-full">
                <div className="w-full h-1.5 bg-mid-gray/20 rounded-full overflow-hidden">
                  <div
                    className="h-full bg-logo-primary rounded-full transition-all duration-300"
                    style={{ width: `${progress ?? 0}%` }}
                  />
                </div>
                <div className="flex items-center justify-between text-xs mt-1 text-text/50">
                  <span>
                    {t("modelSelector.downloading", {
                      percentage: Math.round(progress ?? 0),
                    })}
                  </span>
                  {speed !== undefined && speed > 0 && (
                    <span className="tabular-nums">
                      {t("modelSelector.downloadSpeed", {
                        speed: speed.toFixed(1),
                      })}
                    </span>
                  )}
                </div>
              </div>
            )}
            {verifying && (
              <p className="text-xs text-text/50">
                {t("modelSelector.verifyingGeneric")}
              </p>
            )}
          </div>
        );
      })}
    </div>
  );
};
