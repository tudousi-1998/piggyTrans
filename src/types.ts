export type TranslationProvider = "baidu_general" | "baidu_llm" | "llm";

export type AppSettings = {
  hotkey: string;
  launch_at_login: boolean;
  translation_provider: TranslationProvider;
  ui_body_font_size: number;
};

export type BaiduGeneralSettings = {
  app_id: string;
  app_key: string;
};

export type BaiduLlmSettings = {
  api_url: string;
  app_id: string;
  api_key: string;
};

export type CustomLlmSettings = {
  api_base: string;
  api_key: string;
  model: string;
};

export type AllSettings = {
  general: AppSettings;
  baidu_general: BaiduGeneralSettings;
  baidu_llm: BaiduLlmSettings;
  custom_llm: CustomLlmSettings;
};

export type TranslationResult = {
  original_text: string;
  translated_text: string;
  detected_source: string;
};

export type OverlayOpenPayload = {
  mode: "translate" | "manual" | "permission";
  text: string | null;
  anchor_near_cursor: boolean;
};
