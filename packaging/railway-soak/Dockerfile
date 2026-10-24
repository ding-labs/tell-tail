FROM python:3.11-slim-bullseye@sha256:9e25f400253a5fa3191813d6a67eb801ca1e6f012b3bd2588fa6920b59e3eba6
RUN apt-get update && apt-get install --no-install-recommends -y git procps ca-certificates \
    && rm -rf /var/lib/apt/lists/*
ARG SOURCE_COMMIT=33f2f9ac959350bcd72332f5a62b53480361c933
RUN git init /source && git -C /source remote add origin https://github.com/zuchka/tell-tail.git \
    && git -C /source fetch --depth 1 origin "$SOURCE_COMMIT" \
    && git -C /source checkout --detach FETCH_HEAD \
    && test "$(git -C /source rev-parse HEAD)" = "$SOURCE_COMMIT"
COPY tell-tail BUILD.json identity.json /candidate/
RUN chmod 0555 /candidate/tell-tail
COPY runner.py /runner.py
ENV PYTHONUNBUFFERED=1
CMD ["python", "/runner.py"]
