# Release checklist

- Нет внешних middleware runtime dependency.
- Нет public API growth без обновления `docs/API.ru.md`.
- Все предупреждения описаны в `docs/WARNINGS.ru.md`.
- `npm run check` проходит в Podman-контейнере.
- Benchmark smoke создаёт machine-readable результат.
