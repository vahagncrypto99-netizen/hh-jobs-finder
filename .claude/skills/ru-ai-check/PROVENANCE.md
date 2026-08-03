# Происхождение

Скилл `ru-ai-check` — это [ru-text](https://github.com/talkstream/ru-text)
Арсения Камышева (MIT, см. `LICENSE`), включённый в состав репозитория без
изменений, кроме имени в `SKILL.md` (`ru-text` → `ru-ai-check`).

Зачем в комплекте: `hh-letter` прогоняет через него Gate 1 — оценку письма по
пяти измерениям (типографика, чистота языка, грамотность, структура, точность
для читателя). Без скилла гейт не отработает, поэтому он лежит рядом, а не
ставится отдельно.

Обновить до свежей версии апстрима:

```bash
git clone --depth 1 https://github.com/talkstream/ru-text /tmp/ru-text
rm -rf .claude/skills/ru-ai-check/{SKILL.md,references,agents}
cp -R /tmp/ru-text/skills/ru-text/* .claude/skills/ru-ai-check/
# вернуть имя, иначе скилл не найдётся по вызову из hh-letter
sed -i '' 's/^name: ru-text$/name: ru-ai-check/' .claude/skills/ru-ai-check/SKILL.md
```
