/**
 * Короткое сообщение внизу окна: итог действия, у которого иначе не было бы
 * видимого результата («Убрать пропавшие файлы», обновление по папкам).
 * Ошибки сюда не идут — у них своя полоса.
 */
class NoticeStore {
  text = $state<string | null>(null);
  private timer: ReturnType<typeof setTimeout> | undefined;

  show(text: string, ms = 4_000): void {
    this.text = text;
    clearTimeout(this.timer);
    this.timer = setTimeout(() => (this.text = null), ms);
  }

  hide(): void {
    clearTimeout(this.timer);
    this.text = null;
  }
}

export const notice = new NoticeStore();
