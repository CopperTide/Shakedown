FROM shakedown-sf/base:1.0.0
WORKDIR /app
COPY requirements.txt .
RUN pip install --no-cache-dir --no-index --find-links /opt/wheels -r requirements.txt
COPY storefront/ storefront/
COPY migrations/ migrations/
USER app
EXPOSE 8000
CMD ["python", "-m", "storefront"]
