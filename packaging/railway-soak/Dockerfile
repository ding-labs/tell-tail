FROM python:3.11-slim-bookworm@sha256:f06d01d4ba3f8ad5af45a3ec87df19c1d317d8e8961edc0fabda61811af6f7cf
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
